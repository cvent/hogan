use crate::storage::cache::Cache;
use anyhow::Result;
use itertools::Itertools;

pub struct MultiCache {
    caches: Vec<Box<dyn Cache + Send + Sync>>,
    id: String,
}

impl MultiCache {
    pub fn new(caches: Vec<Box<dyn Cache + Send + Sync>>) -> Self {
        let id = caches.iter().map(|c| c.id()).join("+").to_string();
        MultiCache { caches, id }
    }
}

impl Cache for MultiCache {
    fn id(&self) -> &str {
        &self.id
    }

    fn clean(&self, max_age: usize) -> Result<()> {
        for cache in self.caches.iter() {
            cache.clean(max_age)?
        }
        Ok(())
    }

    fn read_env(
        &self,
        env: &str,
        sha: &str,
    ) -> Result<Option<std::sync::Arc<hogan::config::Environment>>> {
        for (i, cache) in self.caches.iter().enumerate() {
            let result = cache.read_env(env, sha);
            match result {
                Ok(Some(ref environment)) => {
                    for missing_cache in self.caches.iter().take(i) {
                        log::debug!(
                            "Cache miss: Writing environment {} {} to cache {}",
                            env,
                            sha,
                            missing_cache.id()
                        );
                        missing_cache.write_env(env, sha, environment)?;
                    }
                    return result;
                }
                Ok(None) => continue,
                Err(_) => return result,
            }
        }
        Ok(None)
    }

    fn write_env(&self, env: &str, sha: &str, data: &hogan::config::Environment) -> Result<()> {
        for cache in self.caches.iter() {
            cache.write_env(env, sha, data)?
        }
        Ok(())
    }

    fn read_env_listing(
        &self,
        sha: &str,
    ) -> Result<Option<std::sync::Arc<Vec<hogan::config::EnvironmentDescription>>>> {
        for (i, cache) in self.caches.iter().enumerate() {
            let result = cache.read_env_listing(sha);
            match result {
                Ok(Some(ref environment_listing)) => {
                    for missing_cache in self.caches.iter().take(i) {
                        log::debug!(
                            "Cache miss: Writing environment listing {} to cache {}",
                            sha,
                            missing_cache.id()
                        );
                        missing_cache.write_env_listing(sha, environment_listing)?;
                    }
                    return result;
                }
                Ok(None) => continue,
                Err(_) => return result,
            }
        }
        Ok(None)
    }

    fn write_env_listing(
        &self,
        sha: &str,
        data: &[hogan::config::EnvironmentDescription],
    ) -> Result<()> {
        for cache in self.caches.iter() {
            cache.write_env_listing(sha, data)?
        }

        Ok(())
    }
}
