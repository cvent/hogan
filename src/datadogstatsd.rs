use dogstatsd::{Client, Options};
use std::env;
// use std::hash::{Hash, Hasher};
// use std::collections::HashMap;
pub struct  DdMetrics{
    default_tags: [String; 2],
    client: Client
}

impl Default for DdMetrics {
    fn default() -> Self {
        let dd_options = Options::default();
        DdMetrics{
            default_tags: [String::from("service:hogan"), "env:unknow".to_string()],
            client: Client::new(dd_options).unwrap(),
        }
    }
}
impl DdMetrics {
    pub fn new () -> Self {
        let dd_options = Options::default();
        let mut env_tag = String::from("env: ");
        let key = "ENV";
        match env::var(key) {
            Ok(val) => {
                info!("{}: {}", key, val);
                env_tag.push_str(&val);
            }
            Err(e) => info!("couldn't interpret {}: {}", key, e),
        }

        let dd_tags = [String::from("service:hogan"), env_tag];
        DdMetrics{
            default_tags: dd_tags,
            client: Client::new(dd_options).unwrap(),
        }
    }
    pub fn incr(&self, name:&str, url: &str) {
        self.client.incr(name, self.append_url_tag(url).iter())
        .unwrap_or_else(|err| self.error_msg(name, &err.to_string()));
    }

    pub fn decr(&self, name:&str,  url: &str){
        self.client.incr(name, self.append_url_tag(url).iter())
        .unwrap_or_else(|err| self.error_msg(name, &err.to_string()));
    }

    pub fn gauge(&self, name:&str,  url: &str, value: &str){
        self.client.gauge(name, value, self.append_url_tag(url).iter())
        .unwrap_or_else(|err| self.error_msg(name, &err.to_string()));
    }

    fn append_url_tag(&self, url: &str) -> Vec<String> {
        let mut dd_tags = Vec::new();
        dd_tags.extend_from_slice(&self.default_tags);

        let mut url_tag = String::from("request_url: ");
        url_tag.push_str(url);

        dd_tags.push(url_tag);
        dd_tags
    }

    fn error_msg(&self, name: &str, err: &str) {
        info!("{} dd metrics failed with error {}", name, err)
    }

}
