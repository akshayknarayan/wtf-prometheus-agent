//! Express bounds on metrics.

use chrono::{Duration, Utc};
use prometheus_parse::{HistogramCount, Value};

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
    pub fn is_relative(&self) -> bool {
        match self {
            Self::AbsLower(_) | Self::AbsUpper(_) => false,
            Self::RateLower { .. } | Self::RateUpper { .. } => true,
        }
    }

    pub fn check(
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
