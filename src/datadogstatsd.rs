use dogstatsd::{Client, Options};

pub struct  DdMetrics<'a>{
    name: &'a str,
    tags: [String; 3],
    client: Client
}

impl<'a> DdMetrics<'a> {
    pub fn new (name: &'a str, url: &str) -> Self {
        let dd_options = Options::default();
        let mut url_tag = String::from("request_url: ");
        url_tag.push_str(url);
        let dd_tags = [String::from("env:sandbox"), String::from("service:hogan"), url_tag];
        DdMetrics{
            name: name,
            tags: dd_tags,
            client: Client::new(dd_options).unwrap(),
        }
    }
    pub fn incr(&self) {
        // method body would be defined here
        self.client.incr(self.name, self.tags.iter()).unwrap();
    }

    pub fn decr(&self){
        self.client.incr(self.name, self.tags.iter()).unwrap();
    }

    pub fn gauge(&self, value: &str){
        self.client.gauge(self.name, value, self.tags.iter()).unwrap();
    }
}
