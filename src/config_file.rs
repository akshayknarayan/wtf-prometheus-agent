//! A TOML config to specify Prometheus health bit filters.
//!
//! Format of config file:
//! ```rust
//! let cfg = "
//! [prometheus]
//! url = \"http://localhost:9090\"
//!
//! [[prometheus.alerts]]
//! name = \"RabbitmqTooManyUnackMessages\"
//!
//! [[prometheus.alerts]]
//! name = \"KubeStatefulSetReplicasMismatch\"
//! labels = {\"statefulset\" = \"rabbitmq\"}
//!
//! [[elements]]
//! url = \"http://localhost:9419/metrics\"
//!
//! [[elements.bounds]]
//! metric_name = \"rabbitmq_global_messages_unroutable_dropped_total\"
//! bound_type = \"abs_upper\"
//! limit = 1
//! ";
//! wtf_prometheus_agent::parse_config_str(cfg).unwrap();
//! ```

use std::collections::HashMap;
use std::path::Path;

use chrono::Duration;
use color_eyre::{
    eyre::{bail, eyre, Context},
    Report,
};
use serde::Deserialize;

use crate::Bound;

pub fn parse_config(config_file: impl AsRef<Path>) -> Result<Config, Report> {
    let cfg = std::fs::read_to_string(config_file)?;
    parse_config_str(&cfg)
}

pub fn parse_config_str(cfg: &str) -> Result<Config, Report> {
    toml::from_str::<Config>(cfg)
        .wrap_err("TOML file did not match deserialization struct, or was malformed")
}

// rust toml uses serde, so we define structs to deserialize into.
#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    pub prometheus: Prometheus,
    pub elements: Vec<Element>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Prometheus {
    pub url: String,
    pub alerts: Vec<AlertSpec>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct AlertSpec {
    pub name: String,
    pub labels: Option<HashMap<String, String>>,
}

impl From<AlertSpec> for crate::alert::AlertFilter {
    fn from(value: AlertSpec) -> Self {
        crate::alert::AlertFilter {
            name: value.name,
            labels: value.labels.unwrap_or_default(),
        }
    }
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

impl TryFrom<FilterSpec> for crate::element::Filter {
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

        Ok(crate::element::Filter::Exact {
            metric_name: value.metric_name,
            trigger: b,
        })
    }
}
