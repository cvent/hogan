#![allow(clippy::from_over_into)]

use crate::app::datadogstatsd::{CustomMetrics, DdMetrics};
use hogan::config::ConfigDir;
use riker::actors::*;
use std::sync::Arc;
use std::time::Duration;
use std::time::SystemTime;

#[derive(Debug, Clone)]
pub struct ExecuteFetch {}

#[actor(ExecuteFetch)]
#[derive(Debug)]
struct FetchActor {
    config: Arc<ConfigDir>,
    last_updated: SystemTime,
    metrics: Arc<DdMetrics>,
    fetch_delay: u64,
    maintenance_count: u8,
}

impl ActorFactoryArgs<(Arc<ConfigDir>, Arc<DdMetrics>, u64)> for FetchActor {
    fn create_args((config, metrics, fetch_delay): (Arc<ConfigDir>, Arc<DdMetrics>, u64)) -> Self {
        FetchActor {
            config,
            last_updated: SystemTime::now(),
            metrics,
            fetch_delay,
            maintenance_count: 0,
        }
    }
}

impl Actor for FetchActor {
    type Msg = FetchActorMsg;

    fn recv(&mut self, ctx: &Context<Self::Msg>, msg: Self::Msg, sender: Sender) {
        self.receive(ctx, msg, sender);
    }
}

impl Receive<ExecuteFetch> for FetchActor {
    type Msg = FetchActorMsg;

    fn receive(&mut self, _ctx: &Context<Self::Msg>, _msg: ExecuteFetch, _sender: Sender) {
        let start_time = SystemTime::now();
        let fetch_result = self.config.fetch_only("origin");
        if let Ok(elapsed_time) = start_time.elapsed() {
            if let Err(e) = &fetch_result {
                warn!(
                    "Unable to perform scheduled repo fetch. Took: {} ms. Error: {:?}",
                    elapsed_time.as_millis(),
                    e
                );
            } else {
                debug!(
                    "Performed scheduled repo fetch took: {} ms. Poll delay: {} ms",
                    elapsed_time.as_millis(),
                    self.last_updated
                        .elapsed()
                        .unwrap_or_else(|_| Duration::from_secs(0))
                        .as_millis()
                );

                //Only record time of successful fetches
                self.metrics.time(
                    CustomMetrics::FetchTime.into(),
                    None,
                    elapsed_time.as_millis() as i64,
                );
            }
            let counter_tags = if fetch_result.is_err() {
                vec!["time:error".to_string()]
            } else if elapsed_time.as_millis() <= self.fetch_delay as u128 {
                vec!["time:under".to_string()]
            } else {
                vec!["time:over".to_string()]
            };

            self.metrics
                .incr(CustomMetrics::FetchCounter.into(), Some(counter_tags));
        }
        if self.maintenance_count >= 10 {
            self.maintenance_count = 0;
            let maintenance_start = SystemTime::now();
            let maintenance_result = self.config.perform_maintenance();
            if let Ok(elapsed_time) = maintenance_start.elapsed() {
                if let Err(e) = &maintenance_result {
                    warn!(
                        "Unable to perform maintenance on git repo. Took {} ms. Error: {:?}",
                        elapsed_time.as_millis(),
                        e
                    );
                } else {
                    info!(
                        "Performed maintenance on repo took: {} ms",
                        elapsed_time.as_millis()
                    );
                }
                self.metrics.time(
                    CustomMetrics::MaintenanceTime.into(),
                    None,
                    elapsed_time.as_millis() as i64,
                );
            }
        } else {
            self.maintenance_count += 1
        }

        self.last_updated = SystemTime::now();
    }
}

pub fn init_system(
    system: &ActorSystem,
    config: Arc<ConfigDir>,
    metrics: Arc<DdMetrics>,
    fetch_poller_delay: u64,
) {
    let worker = system
        .actor_of_args::<FetchActor, _>("repo-fetch-worker", (config, metrics, fetch_poller_delay))
        .unwrap();

    system.schedule(
        Duration::from_millis(fetch_poller_delay),
        Duration::from_millis(fetch_poller_delay),
        worker.clone(),
        None,
        ExecuteFetch {},
    );

    info!("Scheduled fetch poller for every {} ms", fetch_poller_delay);

    worker.tell(ExecuteFetch {}, None);
}
