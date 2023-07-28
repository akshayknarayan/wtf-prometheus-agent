use chrono::{Duration, Utc};
use color_eyre::eyre::{Context, Report};
use prometheus_parse::{HistogramCount, Sample, Value};
use reqwest::{IntoUrl, Url};
use std::collections::HashMap;

/// Describes when to set health bits on Prometheus metrics
#[derive(Debug, Clone, PartialEq)]
pub enum Filter {
    Exact { metric_name: String, trigger: Bound },
    //Glob { metric_glob: glob::Pattern, trigger: Bound },
    //Regex { metric_re: String, trigger: Bound },
}

/// Bounds for setting metrics using `Filter`s
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Bound {
    AbsLower(f64),
    AbsUpper(f64),
    RateLower {
        min_increment: f64,
        time_period: chrono::Duration,
    },
    RateUpper {
        max_increment: f64,
        time_period: chrono::Duration,
    },
}

impl Bound {
    fn is_relative(&self) -> bool {
        match self {
            Self::AbsLower(_) | Self::AbsUpper(_) => false,
            Self::RateLower { .. } | Self::RateUpper { .. } => true,
        }
    }

    fn check(
        &self,
        value: &Value,
        time: chrono::DateTime<Utc>,
        old_value: Option<&Value>,
        old_time: Option<chrono::DateTime<Utc>>,
    ) -> bool {
        fn diffs(
            value: &Value,
            time: chrono::DateTime<Utc>,
            old_value: Option<&Value>,
            old_time: Option<chrono::DateTime<Utc>>,
        ) -> Option<(f64, Duration)> {
            let num = match value {
                Value::Counter(num) | Value::Gauge(num) => *num,
                _ => return None,
            };

            let old_num = match old_value {
                Some(Value::Counter(num)) | Some(Value::Gauge(num)) => *num,
                _ => return None,
            };

            let increment = num - old_num;
            let dur = time - old_time?;
            Some((increment, dur))
        }

        match self {
            Self::AbsLower(lower_bound) => match value {
                Value::Counter(_) => false,
                Value::Gauge(num) => *num < *lower_bound,
                Value::Histogram(h) => h.iter().any(|HistogramCount { less_than, count }| {
                    less_than < lower_bound && *count > 0.
                }),
                Value::Summary(_) => false,
                _ => false,
            },
            Self::AbsUpper(upper_bound) => match value {
                Value::Counter(num) | Value::Gauge(num) => *num > *upper_bound,
                Value::Histogram(h) => h.iter().any(|HistogramCount { less_than, count }| {
                    less_than > upper_bound && *count > 0.
                }),
                Value::Summary(_) => false,
                _ => false,
            },
            Self::RateLower {
                min_increment,
                time_period,
            } => {
                let (increment, dur) = if let Some((i, d)) = diffs(value, time, old_value, old_time)
                {
                    (i, d)
                } else {
                    return false;
                };

                let rate_thresh = min_increment / (time_period.num_microseconds().unwrap() as f64);
                let obs_rate = increment / (dur.num_microseconds().unwrap() as f64);
                obs_rate < rate_thresh
            }
            Self::RateUpper {
                max_increment,
                time_period,
            } => {
                let (increment, dur) = if let Some((i, d)) = diffs(value, time, old_value, old_time)
                {
                    (i, d)
                } else {
                    return false;
                };

                let rate_thresh = max_increment / (time_period.num_microseconds().unwrap() as f64);
                let obs_rate = increment / (dur.num_microseconds().unwrap() as f64);
                obs_rate > rate_thresh
            }
        }
    }
}

pub struct ElementHealth {
    /// the prometheus-exporting endpoint to query
    url: Url,
    client: reqwest::Client,
    /// the Filters
    filter_set: HashMap<String, Vec<Bound>>,
    /// metric_name -> last observed Sample
    relative_state: HashMap<String, Sample>,
}

impl ElementHealth {
    pub fn new(
        url: impl IntoUrl,
        filter_set: impl IntoIterator<Item = Filter>,
    ) -> Result<Self, Report> {
        Ok(Self {
            url: url.into_url()?,
            client: reqwest::Client::builder().build()?,
            filter_set: filter_set.into_iter().fold(
                Default::default(),
                |mut acc,
                 Filter::Exact {
                     metric_name,
                     trigger,
                 }| {
                    acc.entry(metric_name).or_default().push(trigger);
                    acc
                },
            ),
            relative_state: Default::default(),
        })
    }

    pub async fn check(&mut self) -> Result<Vec<Sample>, Report> {
        Ok(self.check_filters(self.collect_prometheus_metrics().await?))
    }

    async fn collect_prometheus_metrics(&self) -> Result<prometheus_parse::Scrape, Report> {
        let body = self
            .client
            .get(self.url.clone())
            .send()
            .await
            .wrap_err("ElementHealth: Network request to get prometheus metrics endpoint")?
            .text()
            .await
            .wrap_err("ElementHealth: Reading response from prometheus metrics endpoint")?;
        let lines = body.lines().map(|s| Ok(s.to_owned()));

        Ok(prometheus_parse::Scrape::parse(lines)?)
    }

    fn check_filters(&mut self, curr_metrics: prometheus_parse::Scrape) -> Vec<Sample> {
        curr_metrics
            .samples
            .into_iter()
            .filter(|sample| {
                let Sample {
                    ref metric,
                    ref value,
                    ref timestamp,
                    ..
                } = &sample;
                if let Some(bounds) = self.filter_set.get(metric) {
                    bounds.iter().any(|bound| {
                        if bound.is_relative() {
                            let existing = self
                                .relative_state
                                .entry(metric.clone())
                                .or_insert(sample.clone());
                            let old_value = &existing.value;
                            let old_time = existing.timestamp;
                            let res =
                                bound.check(value, *timestamp, Some(old_value), Some(old_time));
                            *existing = sample.clone();
                            res
                        } else {
                            bound.check(value, *timestamp, None, None)
                        }
                    })
                } else {
                    false
                }
            })
            .collect()
    }
}
