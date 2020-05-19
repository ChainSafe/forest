use tide::{Request, Response, StatusCode};
use jsonrpsee::common::MethodCall;
use chain::ChainStore;
use blockstore::BlockStore;
use std::sync::Arc;

struct State <DB: BlockStore>{
    pub chain: Arc<DB>,
}

async fn handle_json_rpc<DB: BlockStore> (mut req: Request<State<DB>>) -> tide::Result {
    let call: MethodCall = req.body_json().await?;
//    req.state().chain.get("");
    Ok(Response::new(StatusCode::Ok).body_json(&call)?)
}

pub async fn start_rpc<DB: BlockStore + Send + Sync + 'static> (chain: Arc<DB>) {
    let mut app = tide::Server::with_state(State {
        chain
    });
    app.at("/api").post(handle_json_rpc);
    app.listen("127.0.0.1:8080").await.unwrap();
}