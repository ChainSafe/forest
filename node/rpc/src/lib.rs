// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod chain_api;
mod sync_api;

use async_std::sync::{RwLock, Sender};
use blockstore::BlockStore;
use chain_sync::{BadBlockCache, SyncState};
use forest_libp2p::NetworkMessage;
use jsonrpc_v2::{Data, MapRouter, RequestObject, Server};
use std::sync::Arc;
use tide::{Request, Response, StatusCode};

/// This is where you store persistant data, or at least access to stateful data.
pub struct RpcState<DB: BlockStore + Send + Sync + 'static> {
    pub store: Arc<DB>,
    pub bad_blocks: Arc<BadBlockCache>,
    pub sync_state: Arc<RwLock<SyncState>>,
    pub network_send: Sender<NetworkMessage>,
    pub network_name: String,
}

async fn handle_json_rpc(mut req: Request<Server<MapRouter>>) -> tide::Result {
    let call: RequestObject = req.body_json().await?;
    let res = req.state().handle(call).await;
    Ok(Response::new(StatusCode::Ok).body_json(&res)?)
}

pub async fn start_rpc<DB: BlockStore + Send + Sync + 'static>(
    state: RpcState<DB>,
    rpc_endpoint: &str,
) {
    use chain_api::*;
    use sync_api::*;
    let rpc = Server::new()
        .with_data(Data::new(state))
        // Chain API
        .with_method("Filecoin.ChainGetMessage", chain_get_message::<DB>)
        .with_method("Filecoin.ChainGetObj", chain_read_obj::<DB>)
        .with_method("Filecoin.ChainHasObj", chain_has_obj::<DB>)
        .with_method("Filecoin.ChainGetBlockMessages", chain_block_messages::<DB>)
        .with_method(
            "Filecoin.ChainGetTipsetByHeight",
            chain_get_tipset_by_height::<DB>,
        )
        .with_method("Filecoin.ChainGetGenesis", chain_get_genesis::<DB>)
        .with_method("Filecoin.ChainTipsetWeight", chain_tipset_weight::<DB>)
        .with_method("Filecoin.ChainGetTipset", chain_get_tipset::<DB>)
        .with_method("Filecoin.GetRandomness", chain_get_randomness::<DB>)
        .with_method("Filecoin.ChainGetBlock", chain_get_block::<DB>)
        .with_method("Filecoin.ChainHead", chain_head::<DB>)
        // Sync API
        .with_method("Filecoin.SyncCheckBad", sync_check_bad::<DB>)
        .with_method("Filecoin.SyncMarkBad", sync_mark_bad::<DB>)
        .with_method("Filecoin.SyncState", sync_state::<DB>)
        .with_method("Filecoin.SyncSubmitBlock", sync_submit_block::<DB>)
        .finish_unwrapped();

    let mut app = tide::Server::with_state(rpc);
    app.at("/rpc/v0").post(handle_json_rpc);
    app.listen(rpc_endpoint).await.unwrap();
}
