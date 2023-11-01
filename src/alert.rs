use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use color_eyre::{
    eyre::{bail, eyre, WrapErr},
    Report,
};
use reqwest::{IntoUrl, Url};

use crate::config_file::Prometheus;

#[derive(Clone, Debug)]
pub struct AlertFilter {
    pub name: String,
    pub labels: HashMap<String, String>,
}

pub struct AlertChecker {
    url: Url,
    client: reqwest::Client,
    alert_set: Vec<AlertFilter>,
}

impl TryFrom<Prometheus> for AlertChecker {
    type Error = Report;
    fn try_from(value: Prometheus) -> Result<Self, Self::Error> {
        Self::new(value.url, value.alerts.into_iter().map(Into::into))
    }
}

impl AlertChecker {
    pub fn new(
        url: impl IntoUrl,
        filters: impl IntoIterator<Item = AlertFilter>,
    ) -> Result<Self, Report> {
        Ok(Self {
            url: url.into_url()?,
            client: reqwest::Client::builder().build()?,
            alert_set: filters.into_iter().collect(),
        })
    }

    pub async fn check(&mut self) -> Result<Vec<Alert>, Report> {
        let body = self
            .client
            .get(self.url.clone())
            .send()
            .await
            .wrap_err("AlertChecker: Could not query alerts endpoint")?
            .text()
            .await
            .wrap_err("AlertChecker: reading response from alerts endpoint")?;
        let AlertsResponse {
            status,
            data: AlertsResponseBody { alerts },
        } = serde_json::from_str(&body)
            .wrap_err(eyre!("AlertChecker: parse response as json: {:?}", body))?;
        if status != "success" {
            bail!("AlertChecker: response indicates error");
        }

        Ok(alerts
            .into_iter()
            .filter(|a| self.alert_set.iter().any(|f| a.check(f)))
            .collect())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct AlertsResponse {
    status: String,
    data: AlertsResponseBody,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct AlertsResponseBody {
    alerts: Vec<Alert>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Alert {
    labels: HashMap<String, String>,
    annotations: HashMap<String, String>,
    state: String,
    active_at: String,
    value: String,
}

impl Alert {
    fn check(&self, filter: &AlertFilter) -> bool {
        match self.labels.get("alertname") {
            None => return false,
            Some(name) if name == &filter.name => (),
            _ => return false,
        }

        self.state == "firing"
            && if !filter.labels.is_empty() {
                filter
                    .labels
                    .iter()
                    .all(|(filter_label_name, filter_label_val)| {
                        self.labels
                            .get(filter_label_name)
                            .is_some_and(|v| filter_label_val == v)
                    })
            } else {
                true
            }
    }
}
