use blocks::BlockHeader;
use blockstore::BlockStore;
use chain::ChainStore;
use cid::json::CidJson;
use message::UnsignedMessage;
use std::sync::Arc;
use tide::{Request, Response, StatusCode};

use jsonrpc_v2::{Data, Params, Server, RequestObject,MapRouter,  Error as JsonRpcError};
//use jsonrpc_v2::*;
struct State<DB: BlockStore + Send + Sync + 'static> {
    pub chain: Arc<DB>,
}

async fn get_chain_message<DB: BlockStore + Send + Sync + 'static>(
    data: Data<State<DB>>,
) -> Result<u64, JsonRpcError> {
    Ok(3u64)
}

async fn cid_i<DB: BlockStore + Send + Sync + 'static>(
    _data: Data<State<DB>>,
    Params(params): Params<(CidJson)>,
) -> Result<CidJson, JsonRpcError> {
    Ok(params)
}

async fn handle_json_rpc(mut req: Request<Server<MapRouter>>) -> tide::Result {
    let call: RequestObject = req.body_json().await?;
    let res = req.state().handle(call).await;
    Ok(Response::new(StatusCode::Ok).body_json(&res)?)
}

pub async fn start_rpc<DB: BlockStore + Send + Sync + 'static>(chain: Arc<DB>) {
    let rpc = Server::new()
        .with_data(Data::new(State { chain }))
        .with_method("Filecoin.ChainGetMessage", get_chain_message::<DB>)
        .with_method("Filecoin.CidI", cid_i::<DB>)
        .finish_unwrapped();
    let mut app = tide::Server::with_state(rpc);
    app.at("/api").post(handle_json_rpc);
    app.listen("127.0.0.1:8080").await.unwrap();
}
