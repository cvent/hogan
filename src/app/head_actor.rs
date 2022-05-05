#![allow(clippy::from_over_into)]

use anyhow::Result;
use futures::executor::block_on;
use futures::future::RemoteHandle;
use hogan::config::ConfigDir;
use hogan::error::HoganError;
use riker::actors::*;
use riker_patterns::ask;
use std::collections::hash_map::HashMap;
use std::sync::Arc;
use std::time::Duration;
use std::time::SystemTime;
use uuid::Uuid;

pub type HeadRequestActor = ActorRef<HeadRequestHolderMsg>;

#[derive(Debug, Clone)]
pub struct HeadQuery {
    branch: String,
}

#[derive(Debug, Clone)]
pub struct HeadResult {
    sha: Result<String, HoganError>,
    branch: String,
    id: Uuid,
}

#[derive(Debug, Clone)]
pub struct PerformQuery {
    branch: String,
    id: Uuid,
}
#[derive(Debug, Clone)]
pub struct QueryTimeout {
    branch: String,
    id: Uuid,
}

#[derive(Debug, Clone)]
pub enum HeadRequestHolderMsg {
    HeadResult(HeadResult),
    HeadQuery(HeadQuery),
    QueryTimeout(QueryTimeout),
}

impl From<QueryTimeout> for HeadRequestHolderMsg {
    fn from(qt: QueryTimeout) -> Self {
        HeadRequestHolderMsg::QueryTimeout(qt)
    }
}

impl From<HeadResult> for HeadRequestHolderMsg {
    fn from(h: HeadResult) -> Self {
        HeadRequestHolderMsg::HeadResult(h)
    }
}

impl From<HeadQuery> for HeadRequestHolderMsg {
    fn from(q: HeadQuery) -> Self {
        HeadRequestHolderMsg::HeadQuery(q)
    }
}

#[derive(Debug)]
struct HeadRequestHolder {
    queries: HashMap<String, Vec<BasicActorRef>>,
    request_ids: HashMap<String, Uuid>,
    request_worker: ActorRef<HeadQueryWorkerMsg>,
}

impl ActorFactoryArgs<ActorRef<HeadQueryWorkerMsg>> for HeadRequestHolder {
    fn create_args(request_worker: ActorRef<HeadQueryWorkerMsg>) -> Self {
        HeadRequestHolder {
            queries: HashMap::new(),
            request_ids: HashMap::new(),
            request_worker,
        }
    }
}

fn get_request_id(holder: &mut HeadRequestHolder, branch: &str) -> Uuid {
    *holder
        .request_ids
        .entry(branch.to_string())
        .or_insert_with(Uuid::new_v4)
}

fn check_request_id(holder: &HeadRequestHolder, branch: &str, id: &Uuid) -> bool {
    if let Some(pending_id) = holder.request_ids.get(branch) {
        pending_id == id
    } else {
        false
    }
}

impl Actor for HeadRequestHolder {
    type Msg = HeadRequestHolderMsg;

    fn recv(&mut self, ctx: &Context<Self::Msg>, msg: Self::Msg, sender: Sender) {
        match msg {
            HeadRequestHolderMsg::QueryTimeout(query) => {
                if check_request_id(self, &query.branch, &query.id) {
                    warn!(
                        "Generating timeout response for {} {}",
                        query.branch, query.id
                    );
                    ctx.myself().tell(
                        HeadResult {
                            id: query.id,
                            branch: query.branch,
                            sha: Err(HoganError::InternalTimeout),
                        },
                        Some(ctx.myself().into()),
                    )
                }
            }
            HeadRequestHolderMsg::HeadQuery(query) => {
                if let Some(requests) = self.queries.get_mut(&query.branch) {
                    requests.push(sender.as_ref().unwrap().clone());
                    debug!("Added request to waiting pool {:?}", query);
                } else {
                    self.queries.insert(
                        query.branch.to_owned(),
                        vec![sender.as_ref().unwrap().clone()],
                    );
                    let request_id = get_request_id(self, &query.branch);

                    let request = PerformQuery {
                        branch: query.branch.to_owned(),
                        id: request_id,
                    };

                    let timeout = QueryTimeout {
                        branch: query.branch.to_owned(),
                        id: request_id,
                    };

                    debug!("Starting head query request {:?}", request);
                    ctx.schedule_once(
                        Duration::from_secs(60),
                        ctx.myself(),
                        Some(ctx.myself().into()),
                        timeout,
                    );
                    self.request_worker.tell(request, Some(ctx.myself().into()));
                }
            }
            HeadRequestHolderMsg::HeadResult(result) => {
                if !check_request_id(self, &result.branch, &result.id) {
                    warn!("Received outdated head response for {}", result.branch);
                    return;
                }

                if let Some(requests) = self.queries.get(&result.branch) {
                    for r in requests {
                        if r.try_tell(result.sha.clone(), Some(ctx.myself().into()))
                            .is_err()
                        {
                            warn!("Unable to return response to {:?} {}", r, result.branch);
                        }
                    }
                    debug!(
                        "Returned head request for {} to {} requesters",
                        result.branch,
                        requests.len()
                    );
                    self.queries.remove(&result.branch);
                    self.request_ids.remove(&result.branch);
                }
            }
        }
    }
}

#[actor(PerformQuery)]
#[derive(Debug)]
struct HeadQueryWorker {
    config: Arc<ConfigDir>,
    last_updated: SystemTime,
    allow_fetch: bool,
}

impl ActorFactoryArgs<(Arc<ConfigDir>, bool)> for HeadQueryWorker {
    fn create_args((config, allow_fetch): (Arc<ConfigDir>, bool)) -> Self {
        HeadQueryWorker {
            config,
            last_updated: SystemTime::now(),
            allow_fetch,
        }
    }
}

impl Actor for HeadQueryWorker {
    type Msg = HeadQueryWorkerMsg;

    fn recv(&mut self, ctx: &Context<Self::Msg>, msg: Self::Msg, sender: Sender) {
        self.receive(ctx, msg, sender);
    }
}

impl Receive<PerformQuery> for HeadQueryWorker {
    type Msg = HeadQueryWorkerMsg;

    fn receive(&mut self, ctx: &Context<Self::Msg>, msg: PerformQuery, sender: Sender) {
        let refresh = match self.last_updated.elapsed() {
            Ok(time_since) => {
                let seconds = time_since.as_secs();
                debug!("Time since last head refresh {}", seconds);
                seconds >= 10
            }
            Err(_) => false,
        };

        let result = self
            .config
            .find_branch_head("origin", &msg.branch, refresh && self.allow_fetch)
            .map_err(|e| e.into());

        let response: HeadRequestHolderMsg = HeadResult {
            branch: msg.branch.to_owned(),
            sha: result,
            id: msg.id,
        }
        .into();

        debug!(
            "Found head result {:?} {} refresh: {}",
            response,
            sender.as_ref().unwrap().path(),
            refresh
        );

        let sender_actor = sender.as_ref().unwrap();

        if sender_actor
            .try_tell(response, Some(ctx.myself().into()))
            .is_err()
        {
            error!("Unable to send response for head query: {}", msg.branch);
        }

        if refresh {
            self.last_updated = SystemTime::now();
        }
    }
}

pub fn init_system(
    sys: &ActorSystem,
    config: Arc<ConfigDir>,
    allow_fetch: bool,
) -> HeadRequestActor {
    let worker: ActorRef<HeadQueryWorkerMsg> = sys
        .actor_of_args::<HeadQueryWorker, _>("query_worker", (config, allow_fetch))
        .unwrap();

    sys.actor_of_args::<HeadRequestHolder, _>("query_holder", worker)
        .unwrap()
}

pub fn request_branch_head(
    sys: &ActorSystem,
    holder: &HeadRequestActor,
    branch: &str,
) -> Result<String> {
    let result: RemoteHandle<Result<String, HoganError>> = ask::ask(
        sys,
        holder,
        HeadQuery {
            branch: branch.to_owned(),
        },
    );

    block_on(result).map_err(|e| e.into())
}
