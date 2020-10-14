use dogstatsd::{Client, Options};
use std::env;

#[derive(Debug)]
pub struct DdMetrics {
    default_tags: Vec<String>,
    client: Client,
    enabled: bool,
}
impl Default for DdMetrics {
    fn default() -> Self {
        DdMetrics::new(true)
    }
}
impl DdMetrics {
    pub fn new(enabled: bool) -> Self {
        let dd_options = Options::default();
        let key = "ENV";
        let env_name = env::var(key).unwrap_or_else(|_| "unknown".to_string());

        info!("Setting up datadog with environment {}", env_name);

        let dd_tags = vec![String::from("service:hogan"), format!("env:{}", env_name)];
        DdMetrics {
            default_tags: dd_tags,
            client: Client::new(dd_options).unwrap(),
            enabled,
        }
    }

    pub fn incr(&self, name: &str, additional_tags: Option<Vec<String>>) {
        if !self.enabled {
            return;
        }
        self.client
            .incr(
                name,
                additional_tags
                    .unwrap_or_default()
                    .iter()
                    .chain(self.default_tags.iter()),
            )
            .unwrap_or_else(|err| self.error_msg(name, &err.to_string()));
    }

    #[allow(dead_code)]
    pub fn decr(&self, name: &str, additional_tags: Option<Vec<String>>) {
        if !self.enabled {
            return;
        }
        self.client
            .decr(
                name,
                additional_tags
                    .unwrap_or_default()
                    .iter()
                    .chain(self.default_tags.iter()),
            )
            .unwrap_or_else(|err| self.error_msg(name, &err.to_string()));
    }

    #[allow(dead_code)]
    pub fn gauge(&self, name: &str, additional_tags: Option<Vec<String>>, value: &str) {
        if !self.enabled {
            return;
        }
        self.client
            .gauge(
                name,
                value,
                additional_tags
                    .unwrap_or_default()
                    .iter()
                    .chain(self.default_tags.iter()),
            )
            .unwrap_or_else(|err| self.error_msg(name, &err.to_string()));
    }

    pub fn time(&self, name: &str, additional_tags: Option<Vec<String>>, value: i64) {
        if !self.enabled {
            return;
        }
        self.client
            .timing(
                name,
                value,
                additional_tags
                    .unwrap_or_default()
                    .iter()
                    .chain(self.default_tags.iter()),
            )
            .unwrap_or_else(|err| self.error_msg(name, &err.to_string()));
    }

    fn error_msg(&self, name: &str, err: &str) {
        warn!("{} dd metrics failed with error {}", name, err)
    }
}

pub enum CustomMetrics {
    Cache,
    RequestTime,
    FetchTime,
    FetchCounter,
}

impl From<CustomMetrics> for &str {
    fn from(m: CustomMetrics) -> Self {
        match m {
            CustomMetrics::Cache => &"hogan.cache",
            CustomMetrics::RequestTime => &"hogan.requests",
            CustomMetrics::FetchTime => &"hogan.fetch",
            CustomMetrics::FetchCounter => &"hogan.fetchcounter",
        }
    }
}
