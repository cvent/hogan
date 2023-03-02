use crate::storage::cache::Cache;
use anyhow::{anyhow, Result};
use hogan::config::Environment;
use hogan::config::EnvironmentDescription;
use lru::LruCache;
use parking_lot::Mutex;
use regex::Regex;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::Duration;

pub struct LruEnvCache {
    env_lru: Mutex<LruCache<String, Arc<Environment>>>,
    env_listing_lru: Mutex<LruCache<String, Arc<Vec<EnvironmentDescription>>>>,
    name: String,
}

impl LruEnvCache {
    pub fn new(id: &str, cache_size: usize) -> Result<Self> {
        let size = match NonZeroUsize::new(cache_size) {
            Some(s) => s,
            None => {
                return Err(anyhow!(
                    "Cache size must be positive. Passed {}",
                    cache_size
                ))
            }
        };

        Ok(LruEnvCache {
            name: id.to_string(),
            env_lru: Mutex::new(LruCache::new(size)),
            env_listing_lru: Mutex::new(LruCache::new(size)),
        })
    }
}

impl Cache for LruEnvCache {
    fn id(&self) -> &str {
        &self.name
    }

    fn clean(&self, _max_age: usize) -> anyhow::Result<()> {
        Ok(())
    }

    fn read_env(&self, env: &str, sha: &str) -> Result<Option<Arc<Environment>>> {
        if let Some(mut cache) = self.env_lru.try_lock_for(Duration::from_secs(15)) {
            let key_regex = gen_env_regex(env, sha);
            if let Some(key) = cache
                .iter()
                .filter_map(|(k, _)| if key_regex.is_match(k) { Some(k) } else { None })
                .next()
                .cloned()
            {
                //We have to explicitly get the key out of the may for the LRU to work
                Ok(cache.get(&key).cloned())
            } else {
                Ok(None)
            }
        } else {
            Err(anyhow!("Unable to acquire Env lock"))
        }
    }

    fn write_env(&self, env: &str, sha: &str, data: &Environment) -> anyhow::Result<()> {
        if let Some(mut cache) = self.env_lru.try_lock_for(Duration::from_secs(15)) {
            let key = gen_env_key(env, sha);
            cache.put(key, Arc::new(data.clone()));
            Ok(())
        } else {
            Err(anyhow!("Unable to acquire Env lock"))
        }
    }

    fn read_env_listing(&self, sha: &str) -> Result<Option<Arc<Vec<EnvironmentDescription>>>> {
        if let Some(mut cache) = self.env_listing_lru.try_lock_for(Duration::from_secs(15)) {
            let key_regex = gen_env_listing_regex(sha);
            if let Some(key) = cache
                .iter()
                .filter_map(|(k, _)| if key_regex.is_match(k) { Some(k) } else { None })
                .next()
                .cloned()
            {
                //We have to explicitly get the key out of the may for the LRU to work
                Ok(cache.get(&key).cloned())
            } else {
                Ok(None)
            }
        } else {
            Err(anyhow!("Unable to acquire Env Listing lock"))
        }
    }

    fn write_env_listing(&self, sha: &str, data: &[EnvironmentDescription]) -> Result<()> {
        if let Some(mut cache) = self.env_listing_lru.try_lock_for(Duration::from_secs(15)) {
            let key = gen_env_listing_key(sha);
            cache.put(key, Arc::new(data.to_vec()));
            Ok(())
        } else {
            Err(anyhow!("Unable to acquire Env Listing lock"))
        }
    }
}

fn gen_env_key(env: &str, sha: &str) -> String {
    format!("{}::{}", sha, env)
}

fn gen_env_regex(env: &str, sha: &str) -> Regex {
    Regex::new(&format!("^{}.*::{}$", sha, env)).unwrap()
}

fn gen_env_listing_key(sha: &str) -> String {
    sha.to_string()
}

fn gen_env_listing_regex(sha: &str) -> Regex {
    Regex::new(&format!("^{}.*$", sha)).unwrap()
}
