use crate::app::datadogstatsd::CustomMetrics;
use crate::app::datadogstatsd::DdMetrics;
use anyhow::Result;
use compression::prelude::*;
use hogan::config::Environment;
use hogan::config::EnvironmentDescription;
use riker::actors::*;
use serde::Deserialize;
use serde::Serialize;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

pub trait Cache {
    fn id(&self) -> &str;
    fn clean(&self, max_age: usize) -> Result<()>;
    fn read_env(&self, env: &str, sha: &str) -> Result<Option<Arc<Environment>>>;
    fn write_env(&self, env: &str, sha: &str, data: &Environment) -> Result<()>;
    fn read_env_listing(&self, sha: &str) -> Result<Option<Arc<Vec<EnvironmentDescription>>>>;
    fn write_env_listing(&self, sha: &str, data: &[EnvironmentDescription]) -> Result<()>;
}

#[derive(Default, Serialize, Deserialize, Debug)]
pub struct WritableEnvironment {
    pub config_data: String,
    pub environment: String,
    pub environment_type: Option<String>,
}

impl From<&Environment> for WritableEnvironment {
    fn from(environment: &Environment) -> Self {
        WritableEnvironment {
            config_data: environment.config_data.to_string(),
            environment: environment.environment.to_owned(),
            environment_type: environment.environment_type.to_owned(),
        }
    }
}

impl From<WritableEnvironment> for Environment {
    fn from(environment: WritableEnvironment) -> Self {
        Environment {
            config_data: serde_json::from_str(&environment.config_data).unwrap(),
            environment: environment.environment.to_owned(),
            environment_type: environment.environment_type.to_owned(),
        }
    }
}

#[derive(Default, Serialize, Deserialize, Debug)]
pub struct WritableEnvironmentListing {
    pub environments: Vec<EnvironmentDescription>,
}

impl From<&[EnvironmentDescription]> for WritableEnvironmentListing {
    fn from(environments: &[EnvironmentDescription]) -> Self {
        Self {
            environments: environments.to_owned(),
        }
    }
}

fn compress_data(data: Vec<u8>) -> Result<Vec<u8>> {
    let compressed_data = data
        .into_iter()
        .encode(&mut BZip2Encoder::new(6), Action::Finish)
        .collect::<Result<Vec<_>, _>>()?;

    Ok(compressed_data)
}

fn decompress_data(data: Vec<u8>) -> Result<Vec<u8>> {
    let decompressed_data = data
        .into_iter()
        .decode(&mut BZip2Decoder::new())
        .collect::<Result<Vec<_>, _>>()?;

    Ok(decompressed_data)
}

pub fn serialize_env(data: &Environment) -> Result<Vec<u8>> {
    let writable_data: WritableEnvironment = data.into();
    let encoded_data = bincode::serialize(&writable_data)?;
    let compressed_data = compress_data(encoded_data)?;

    Ok(compressed_data)
}

pub fn deserialize_env(data: Vec<u8>) -> Result<Environment> {
    let decompressed_data = decompress_data(data)?;
    let decoded: WritableEnvironment = match bincode::deserialize(&decompressed_data) {
        Ok(environment) => environment,
        Err(e) => {
            return Err(e.into());
        }
    };
    Ok(decoded.into())
}

pub fn serialize_env_listing(data: &[EnvironmentDescription]) -> Result<Vec<u8>> {
    let writable_data: WritableEnvironmentListing = data.into();
    let encoded_data = bincode::serialize(&writable_data)?;
    let compressed_data = compress_data(encoded_data)?;
    Ok(compressed_data)
}

pub fn deserialize_env_listing(data: Vec<u8>) -> Result<Vec<EnvironmentDescription>> {
    let decompressed_data = decompress_data(data)?;
    let decoded: WritableEnvironmentListing = match bincode::deserialize(&decompressed_data) {
        Ok(environment) => environment,
        Err(e) => return Err(e.into()),
    };
    Ok(decoded.environments)
}

#[derive(Debug, Clone)]
pub struct ExecuteCleanup {}

type CacheBox = Arc<Box<dyn Cache + Sync + Send>>;

#[actor(ExecuteCleanup)]
pub struct CleanupActor {
    caches: Vec<CacheBox>,
    max_age: usize,
    metrics: Arc<DdMetrics>,
}

impl ActorFactoryArgs<(Vec<CacheBox>, usize, Arc<DdMetrics>)> for CleanupActor {
    fn create_args((caches, max_age, metrics): (Vec<CacheBox>, usize, Arc<DdMetrics>)) -> Self {
        CleanupActor {
            caches,
            max_age,
            metrics,
        }
    }
}

impl Actor for CleanupActor {
    type Msg = CleanupActorMsg;

    fn recv(&mut self, ctx: &Context<Self::Msg>, msg: Self::Msg, sender: Sender) {
        self.receive(ctx, msg, sender);
    }
}

impl Receive<ExecuteCleanup> for CleanupActor {
    type Msg = CleanupActorMsg;

    fn receive(&mut self, _ctx: &Context<Self::Msg>, _msg: ExecuteCleanup, _sender: Sender) {
        let now = SystemTime::now();
        for cache in self.caches.iter() {
            match cache.clean(self.max_age) {
                Ok(()) => {
                    let duration = now.elapsed().unwrap_or(Duration::from_millis(0));
                    info!(
                        "Cleaned entries from the {} cache older than {} days. Time {} ms",
                        cache.id(),
                        self.max_age,
                        duration.as_millis()
                    );
                    self.metrics.time(
                        CustomMetrics::DbCleanup.into(),
                        None,
                        duration.as_millis() as i64,
                    );
                }
                Err(err) => {
                    error!("Unable to clean the {} cache: {:?}", cache.id(), err);
                }
            }
        }
    }
}

impl CleanupActor {
    pub fn init_db_cleanup_system(
        system: &ActorSystem,
        caches: &[CacheBox],
        max_age: usize,
        metrics: Arc<DdMetrics>,
    ) {
        let cleanup_poller_delay = 24 * 60 * 60; //1 day
        let worker = system
            .actor_of_args::<CleanupActor, _>(
                "db-cleanup-worker",
                (caches.to_owned(), max_age, metrics),
            )
            .unwrap();

        system.schedule(
            Duration::from_secs(cleanup_poller_delay),
            Duration::from_secs(cleanup_poller_delay),
            worker.clone(),
            None,
            ExecuteCleanup {},
        );

        info!(
            "Scheduled db cleanup poller for every {} s",
            cleanup_poller_delay
        );

        worker.tell(ExecuteCleanup {}, None);
    }
}
