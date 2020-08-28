use hogan::error::HoganError;
use riker::actors::*;
use riker_patterns::ask;
use std::collections::hash_map::HashMap;
use hogan::config::ConfigDir;

#[derive(Debug, Clone)]
pub struct HeadQuery {
    branch: String,
}

#[derive(Debug, Clone)]
pub struct HeadResult {
    sha: Result<String, HoganError>,
    branch: String,
}

#[derive(Debug, Clone)]
pub struct PerformQuery {
    branch: String,
}

#[actor(HeadResult, HeadQuery)]
#[derive(Debug)]
struct HeadRequestHolder {
    queries: HashMap<String, Vec<BasicActorRef>>,
    request_worker: BasicActorRef
}

impl Actor for HeadRequestHolder {
    type Msg = HeadRequestHolderMsg;

    fn recv(&mut self, ctx: &Context<Self::Msg>, msg: Self::Msg, sender: Sender) {
        self.receive(ctx, msg, sender);
    }
}

impl Receive<HeadQuery> for HeadRequestHolder {
    type Msg = HeadRequestHolderMsg;

    fn receive(&mut self, ctx: &Context<Self::Msg>, msg: HeadQuery, sender: Sender) {
        todo!()
    }
}

impl Receive<HeadResult> for HeadRequestHolder {
    type Msg = HeadRequestHolderMsg;

    fn receive(&mut self, ctx: &Context<Self::Msg>, msg: HeadResult, sender: Sender) {
        todo!()
    }
}

#[actor(PerformQuery)]
#[derive(Debug)]
struct HeadQueryWorker {
    config: ConfigDir
};

impl Actor for HeadQueryWorker {
    type Msg = HeadQueryWorkerMsg;

    fn recv(&mut self, ctx: &Context<Self::Msg>, msg: Self::Msg, sender: Sender) {
        self.receive(ctx, msg, sender);
    }
}

impl Receive<PerformQuery> for HeadQueryWorker {
    type Msg = HeadQueryWorkerMsg;

    fn receive(&mut self, ctx: &Context<Self::Msg>, msg: PerformQuery, sender: Sender) {
        todo!()
    }
}
