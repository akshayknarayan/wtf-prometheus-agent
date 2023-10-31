//! Manually monitor individual Prometheus-compatible endpoints

use color_eyre::eyre::{Context, Report};
use prometheus_parse::Sample;
use reqwest::{IntoUrl, Url};
use std::collections::HashMap;

use crate::{config_file, Bound};

/// Describes when to set health bits on Prometheus metrics
#[derive(Debug, Clone, PartialEq)]
pub enum Filter {
    Exact { metric_name: String, trigger: Bound },
    //Glob { metric_glob: glob::Pattern, trigger: Bound },
    //Regex { metric_re: String, trigger: Bound },
}

/// An agent responsible for monitoring a single Prometheus endpoint and returning anomalous
/// metrics reports according to the specified filters.
pub struct ElementHealth {
    /// the prometheus-exporting endpoint to query
    url: Url,
    client: reqwest::Client,
    /// the Filters to check
    // TODO HashMap won't work if Filters can be globs or regexes, unless we do some pre-processing.
    filter_set: HashMap<String, Vec<Bound>>,
    /// metric_name -> last observed Sample
    relative_state: HashMap<String, Sample>,
}

impl TryFrom<config_file::Element> for ElementHealth {
    type Error = Report;
    fn try_from(value: config_file::Element) -> Result<Self, Self::Error> {
        Self::new(
            value.url,
            value
                .bounds
                .into_iter()
                .map(|b| b.try_into())
                .collect::<Result<Vec<Filter>, _>>()?,
        )
    }
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
