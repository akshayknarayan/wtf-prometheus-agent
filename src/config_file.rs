//! A TOML config to specify Prometheus health bit filters.
//!
//! Format of config file:
//! ```toml
//! [[elements]]
//! url = "http://localhost:9419/metrics"
//!
//! [[elements.bounds]]
//! # health bit set when a message is dropped (upper bound)
//! metric_name = "rabbitmq_global_messages_unroutable_dropped_total",
//! bound_type = "abs_upper",
//! limit = 1
//!
//! [[elements.bounds]]
//! # health bit set when there are no queues (lower bound)
//! metric_name = "rabbitmq_queues",
//! bound_type = "abs_lower",
//! limit = 1
//!
//! [[elements.bounds]]
//! # health bit set when rabbitmq's total memory allocated increases by 1MB within a minute
//! metric_name = "erlang_vm_memory_processes_bytes_total"
//! bound_type = "rate_upper",
//! limit = 1000000
//! period = "1m"
//!
//! [[elements]]
//! url = "..."
//! ```

use std::path::Path;

use chrono::Duration;
use color_eyre::{
    eyre::{bail, eyre, Context},
    Report,
};
use serde::Deserialize;

use crate::{Bound, Filter};

pub fn parse_config(config_file: impl AsRef<Path>) -> Result<Vec<Element>, Report> {
    let cfg = std::fs::read_to_string(config_file)?;
    Ok(toml::from_str::<Elements>(&cfg)
        .wrap_err("TOML file did not match deserialization struct, or was malformed")?
        .elements)
}

// rust toml uses serde, so we define structs to deserialize into.
#[derive(Clone, Debug, Deserialize)]
pub struct Elements {
    elements: Vec<Element>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Element {
    pub url: String,
    pub bounds: Vec<FilterSpec>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct FilterSpec {
    metric_name: String,
    bound_type: String,
    limit: f64,
    #[serde(
        default,
        deserialize_with = "duration_str::deserialize_option_duration_chrono"
    )]
    period: Option<Duration>,
}

impl TryFrom<FilterSpec> for crate::Filter {
    type Error = Report;
    fn try_from(value: FilterSpec) -> Result<Self, Self::Error> {
        let bound_type = value.bound_type.to_lowercase();
        let b = match bound_type.as_str() {
            "abs_lower" => Bound::AbsLower(value.limit),
            "abs_upper" => Bound::AbsUpper(value.limit),
            "rate_lower" => Bound::RateLower {
                min_increment: value.limit,
                time_period: value
                    .period
                    .ok_or_else(|| eyre!("RateLower bound requires time period"))?,
            },
            "rate_upper" => Bound::RateUpper {
                max_increment: value.limit,
                time_period: value
                    .period
                    .ok_or_else(|| eyre!("RateUpper bound requires time period"))?,
            },
            s => bail!("Unsupported bound type {:?}", s),
        };

        Ok(Filter::Exact {
            metric_name: value.metric_name,
            trigger: b,
        })
    }
}
