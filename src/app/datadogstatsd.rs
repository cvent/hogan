use dogstatsd::{Client, Options};
use std::env;

pub struct DdMetrics {
    default_tags: Vec<String>,
    client: Client,
}
impl Default for DdMetrics {
    fn default() -> Self {
        DdMetrics::new()
    }
}
impl DdMetrics {
    pub fn new() -> Self {
        let dd_options = Options::default();
        let key = "ENV";
        let env_name = env::var(key).unwrap_or_else(|_| "unknown".to_string());

        info!("Setting up datadog with environment {}", env_name);

        let dd_tags = vec![String::from("service:hogan"), format!("env:{}", env_name)];
        DdMetrics {
            default_tags: dd_tags,
            client: Client::new(dd_options).unwrap(),
        }
    }
    pub fn incr(&self, name: &str, additional_tags: Option<Vec<String>>) {
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

    pub fn decr(&self, name: &str, additional_tags: Option<Vec<String>>) {
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

    pub fn gauge(&self, name: &str, additional_tags: Option<Vec<String>>, value: &str) {
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
    CacheMiss,
    CacheHit,
    RequestTime,
}

impl CustomMetrics {
    pub fn metrics_name(self) -> &'static str {
        match self {
            CustomMetrics::CacheMiss => "hogan.cache_miss.counter",
            CustomMetrics::CacheHit => "hogan.cache_hit.counter",
            CustomMetrics::RequestTime => "hogan.requests",
        }
    }
}
