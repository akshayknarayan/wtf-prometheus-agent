use std::collections::HashMap;

use color_eyre::{
    eyre::{bail, eyre, WrapErr},
    Report,
};
use reqwest::{IntoUrl, Url};

use crate::config_file::Prometheus;

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
}
