use crossbeam_channel::{unbounded, Receiver, Sender};
use failure::Error;
use hogan::config::ConfigDir;
use std::collections::hash_map::HashMap;
use std::sync::Arc;
use std::thread;
use threadpool::ThreadPool;

#[derive(Debug, Clone)]
pub enum HeadRequest {
    Query {
        result_channel: Sender<Option<String>>,
        branch: String,
    },
    Result {
        sha: Option<String>,
        branch: String,
    },
}

impl HeadRequest {
    fn new_query(branch: &str) -> (Receiver<Option<String>>, Self) {
        let (s, r) = unbounded();
        let hr = HeadRequest::Query {
            result_channel: s,
            branch: branch.to_owned(),
        };
        (r, hr)
    }

    fn new_result(branch: &str, sha: &str) -> Self {
        HeadRequest::Result {
            sha: Some(sha.to_owned()),
            branch: branch.to_owned(),
        }
    }

    fn new_empty_result(branch: &str) -> Self {
        HeadRequest::Result {
            sha: None,
            branch: branch.to_owned(),
        }
    }
}

fn head_query(sender: Sender<HeadRequest>, config: Arc<ConfigDir>, branch: String) {
    let result = match config.find_branch_head(&"origin", &branch) {
        Some(sha) => HeadRequest::new_result(&branch, &sha),
        None => HeadRequest::new_empty_result(&branch),
    };
    match sender.send(result) {
        Ok(()) => {}
        Err(e) => warn!("Unable to send the result for {} {:?}", branch, e),
    }
}

fn worker(sender: Sender<HeadRequest>, receiver: Receiver<HeadRequest>, config: Arc<ConfigDir>) {
    let tp = ThreadPool::new(4);
    let mut head_requests: HashMap<String, Vec<Sender<Option<String>>>> = HashMap::new();
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

pub fn request_branch_head(
    sender: &Sender<HeadRequest>,
    branch: &str,
) -> Result<Option<String>, Error> {
    let (return_chan, request) = HeadRequest::new_query(branch);
    match sender.send(request) {
        Ok(()) => match return_chan.recv_timeout(std::time::Duration::from_secs(60)) {
            Ok(result) => Ok(result),
            Err(e) => Err(e.into()),
        },
        Err(e) => {
            warn!("Unable to send head request {} {:?}", branch, e);
            Err(e.into())
        }
    }
}
