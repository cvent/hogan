use super::cache;
use crate::storage::cache::Cache;
use anyhow::Result;
use hogan::config::Environment;
use hogan::config::EnvironmentDescription;
use redis;

use redis::Commands as _;
use std::sync::Arc;

pub struct RedisCache {
    client: redis::Client,
    id: String,
    ttl: usize,
}

impl RedisCache {
    pub fn new(id: &str, connection_string: &str, ttl: usize) -> Result<Self> {
        let client = redis::Client::open(connection_string)?;
        Ok(RedisCache {
            client,
            id: id.to_owned(),
            ttl,
        })
    }
}

impl Cache for RedisCache {
    fn id(&self) -> &str {
        &self.id
    }

    fn clean(&self, _max_age: usize) -> Result<()> {
        Ok(())
    }

    fn read_env(&self, env: &str, sha: &str) -> Result<Option<Arc<Environment>>> {
        let key = gen_env_key(sha, env);
        let mut connection = self.client.get_connection()?;
        let serialized_data: Vec<u8> = match connection.get(&key) {
            Ok(data) => {
                connection.expire(&key, self.ttl.try_into()?)?;
                data
            }
            Err(_e) => return Ok(None),
        };

        let data = cache::deserialize_env(serialized_data)?;

        Ok(Some(Arc::new(data)))
    }

    fn write_env(&self, env: &str, sha: &str, data: &Environment) -> Result<()> {
        let key = gen_env_key(sha, env);
        let serialized_data = cache::serialize_env(data)?;
        let mut connection = self.client.get_connection()?;

        let opts = redis::SetOptions::default().with_expiration(redis::SetExpiry::EX(self.ttl));

        connection.set_options(&key, serialized_data, opts)?;
        debug!("Wrote env {} to redis.", key);
        Ok(())
    }

    fn read_env_listing(&self, sha: &str) -> Result<Option<Arc<Vec<EnvironmentDescription>>>> {
        let key = gen_env_listing_key(sha);
        let mut connection = self.client.get_connection()?;
        let serialized_data: Vec<u8> = match connection.get(&key) {
            Ok(data) => {
                connection.expire(&key, self.ttl.try_into()?)?;
                data
            }
            Err(_e) => return Ok(None),
        };
        let data = cache::deserialize_env_listing(serialized_data)?;
        Ok(Some(Arc::new(data)))
    }

    fn write_env_listing(&self, sha: &str, data: &[EnvironmentDescription]) -> Result<()> {
        let key = gen_env_listing_key(sha);
        let serialized_data = cache::serialize_env_listing(data)?;
        let mut connection = self.client.get_connection()?;

        let opts = redis::SetOptions::default().with_expiration(redis::SetExpiry::EX(self.ttl));
        connection.set_options(key, serialized_data, opts)?;
        Ok(())
    }
}

fn gen_env_key(sha: &str, env: &str) -> String {
    format!("env::{}::{}", sha, env)
}

fn gen_env_listing_key(sha: &str) -> String {
    format!("listing::{}", sha)
}
