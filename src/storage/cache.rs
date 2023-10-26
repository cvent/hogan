use crate::app::datadogstatsd::CustomMetrics;
use crate::app::datadogstatsd::DdMetrics;
use anyhow::Result;
use hogan::config::Environment;
use hogan::config::EnvironmentDescription;
use riker::actors::*;
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
