use anyhow::Context;
use crossbeam_channel::{unbounded, Receiver, Sender};
use hogan::config::ConfigDir;
use hogan::error::HoganError;
use std::collections::hash_map::HashMap;
use std::sync::Arc;
use std::thread;
use threadpool::ThreadPool;

#[derive(Debug, Clone)]
pub enum HeadRequest {
    Query {
        result_channel: Sender<Result<String, HoganError>>,
        branch: String,
    },
    Result {
        sha: Result<String, HoganError>,
        branch: String,
    },
}

impl HeadRequest {
    fn new_query(branch: &str) -> (Receiver<Result<String, HoganError>>, Self) {
        let (s, r) = unbounded();
        let hr = HeadRequest::Query {
            result_channel: s,
            branch: branch.to_owned(),
        };
        (r, hr)
    }

    fn new_result(branch: &str, sha: Result<String, HoganError>) -> Self {
        HeadRequest::Result {
            sha,
            branch: branch.to_owned(),
        }
    }
}

fn head_query(sender: Sender<HeadRequest>, config: Arc<ConfigDir>, branch: String) {
    let result = HeadRequest::new_result(
        &branch,
        config
            .find_branch_head(&"origin", &branch)
            .map_err(|e| e.into()),
    );

    match sender.send(result) {
        Ok(()) => {}
        Err(e) => warn!("Unable to send the result for {} {:?}", branch, e),
    }
}

fn worker(sender: Sender<HeadRequest>, receiver: Receiver<HeadRequest>, config: Arc<ConfigDir>) {
    let tp = ThreadPool::new(4);
    let mut head_requests: HashMap<String, Vec<Sender<Result<String, HoganError>>>> =
        HashMap::new();
    info!("Started head request worker");
    loop {
        let msg = match receiver.recv() {
            Ok(msg) => msg,
            Err(e) => {
                info!("Stopping head worker. Received: {:?}", e);
                break;
            }
        };

        match msg {
            HeadRequest::Result { sha, branch } => {
                if head_requests.contains_key(&branch) {
                    let requests = head_requests.get(&branch).unwrap();
                    for r in requests {
                        if let Err(e) = r.try_send(sha.clone()) {
                            warn!(
                                "Unable to return head response {:?} {} {:?}",
                                e, branch, sha
                            );
                        }
                    }
                    debug!(
                        "Returned {} head requests for {} {:?}",
                        requests.len(),
                        branch,
                        sha
                    );
                    head_requests.remove(&branch);
                } else {
                    warn!("Received lost head result for {} -> {:?}", branch, sha);
                }
            }
            HeadRequest::Query {
                result_channel,
                branch,
            } => {
                if head_requests.contains_key(&branch) {
                    let requests = head_requests.get_mut(&branch).unwrap();
                    debug!("Adding request for {} head", branch);
                    requests.push(result_channel);
                } else {
                    debug!("New branch head request {}", branch);
                    let request = vec![result_channel];
                    head_requests.insert(branch.clone(), request);
                    let new_sender = sender.clone();
                    let new_config = config.clone();
                    tp.execute(move || head_query(new_sender, new_config, branch));
                }
            }
        }
    }
}

pub fn init_head(config: Arc<ConfigDir>) -> Sender<HeadRequest> {
    let (s, r) = unbounded();
    let worker_sender = s.clone();
    thread::spawn(move || worker(worker_sender, r, config));
    s
}

pub fn request_branch_head(sender: &Sender<HeadRequest>, branch: &str) -> anyhow::Result<String> {
    let (return_chan, request) = HeadRequest::new_query(branch);
    match sender.send(request) {
        Ok(()) => match return_chan.recv_timeout(std::time::Duration::from_secs(60)) {
            Ok(result) => result.with_context(|| format!("Querying for head of {}", branch)),
            Err(e) => Err(HoganError::InternalTimeout.into()),
        },
        Err(e) => {
            warn!("Unable to send head request {} {:?}", branch, e);
            Err(HoganError::UnknownError {
                msg: "Unable to send head query".to_string(),
            }
            .into())
        }
    }
}
