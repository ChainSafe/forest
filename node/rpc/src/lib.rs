// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod chain_api;

use blockstore::BlockStore;
use jsonrpc_v2::{Data, MapRouter, RequestObject, Server};
use std::sync::Arc;
use tide::{Request, Response, StatusCode};

/// This is where you store persistant data, or at least access to stateful data.
pub struct State<DB: BlockStore + Send + Sync + 'static> {
    pub store: Arc<DB>,
}

async fn handle_json_rpc(mut req: Request<Server<MapRouter>>) -> tide::Result {
    let call: RequestObject = req.body_json().await?;
    let res = req.state().handle(call).await;
    Ok(Response::new(StatusCode::Ok).body_json(&res)?)
}

pub async fn start_rpc<DB: BlockStore + Send + Sync + 'static>(store: Arc<DB>) {
    let rpc = Server::new()
        .with_data(Data::new(State { store }))
        .with_method(
            "Filecoin.ChainGetMessage",
            chain_api::chain_get_message::<DB>,
        )
        .with_method("Filecoin.ChainGetObj", chain_api::chain_read_obj::<DB>)
        .with_method("Filecoin.ChainHasObj", chain_api::chain_has_obj::<DB>)
        .with_method(
            "Filecoin.ChainGetBlockMessages",
            chain_api::chain_block_messages::<DB>,
        )
        .with_method(
            "Filecoin.ChainGetTipsetByHeight",
            chain_api::chain_get_tipset_by_height::<DB>,
        )
        .with_method(
            "Filecoin.ChainGetGenesis",
            chain_api::chain_get_genesis::<DB>,
        )
        .with_method(
            "Filecoin.ChainTipsetWeight",
            chain_api::chain_tipset_weight::<DB>,
        )
        .with_method("Filecoin.ChainGetTipset", chain_api::chain_get_tipset::<DB>)
        .with_method(
            "Filecoin.GetRandomness",
            chain_api::chain_get_randomness::<DB>,
        )
        .with_method("Filecoin.ChainGetBlock", chain_api::chain_get_block::<DB>)
        .with_method("Filecoin.ChainHead", chain_api::chain_head::<DB>)
        .finish_unwrapped();
    let mut app = tide::Server::with_state(rpc);
    app.at("/api").post(handle_json_rpc);
    app.listen("127.0.0.1:8080").await.unwrap();
}
