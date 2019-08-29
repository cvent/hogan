use crate::config::Environment;
use crossbeam_channel::{Receiver, Sender};
use lru_time_cache::LruCache;
use std::sync::Arc;
use std::thread;

pub enum Response {
    ShaQuery {
        sha: String,
        environments: Vec<Environment>,
    },
}

pub enum Message {
    UpdateCache {
        sha: String,
        environments: Vec<Environment>,
    },
    RequestSha {
        sha: String,
        response: Sender<Response>,
    },
    Quit,
}
//TODO: Write cache worker
fn init_cache_worker(
    work: Receiver<Message>,
    responses: Sender<Response>,
    capacity: Option<usize>,
) {
    thread::spawn(move || {
        let mut cache =
            LruCache::<String, Arc<Vec<Environment>>>::with_capacity(capacity.unwrap_or(512));
        loop {
            match work.recv() {
                Ok(msg) => match msg {
                    Message::RequestSha { sha, response } => if let Some(envs) = cache.get(&sha) {},
                    Message::Quit => {
                        info!("Stopping the cache worker");
                        break;
                    }
                    Message::UpdateCache { sha, environments } => (),
                },
                Err(e) => {
                    error!("Received error on cache worker. Shutting down {:?}", e);
                    break;
                }
            }
        }
    });
}

//TODO: Wirte git worker
