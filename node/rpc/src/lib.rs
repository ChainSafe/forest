// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod chain_api;
mod wallet_api;

pub use wallet_api::*;

use async_std::sync::RwLock;
use blockstore::BlockStore;
use jsonrpc_v2::{Data, MapRouter, RequestObject, Server};
use std::sync::Arc;
use tide::{Request, Response, StatusCode};
use wallet::KeyStore;

/// This is where you store persistant data, or at least access to stateful data.
pub struct State<DB: BlockStore + Send + Sync + 'static, T: KeyStore + Send + Sync + 'static> {
    pub store: Arc<DB>,
    pub keystore: Arc<RwLock<T>>,
}

async fn handle_json_rpc(mut req: Request<Server<MapRouter>>) -> tide::Result {
    let call: RequestObject = req.body_json().await?;
    let res = req.state().handle(call).await;
    Ok(Response::new(StatusCode::Ok).body_json(&res)?)
}

pub async fn start_rpc<
    DB: BlockStore + Send + Sync + 'static,
    T: KeyStore + Send + Sync + 'static,
>(
    store: Arc<DB>,
    keystore: Arc<RwLock<T>>,
) {
    let rpc = Server::new()
        .with_data(Data::new(State { store, keystore }))
        .with_method(
            "Filecoin.ChainGetMessage",
            chain_api::chain_get_message::<DB, T>,
        )
        .with_method("Filecoin.ChainGetObj", chain_api::chain_read_obj::<DB, T>)
        .with_method("Filecoin.ChainHasObj", chain_api::chain_has_obj::<DB, T>)
        .with_method(
            "Filecoin.ChainGetBlockMessages",
            chain_api::chain_block_messages::<DB, T>,
        )
        .with_method(
            "Filecoin.ChainGetTipsetByHeight",
            chain_api::chain_get_tipset_by_height::<DB, T>,
        )
        .with_method(
            "Filecoin.ChainGetGenesis",
            chain_api::chain_get_genesis::<DB, T>,
        )
        .with_method(
            "Filecoin.ChainTipsetWeight",
            chain_api::chain_tipset_weight::<DB, T>,
        )
        .with_method(
            "Filecoin.ChainGetTipset",
            chain_api::chain_get_tipset::<DB, T>,
        )
        .with_method(
            "Filecoin.GetRandomness",
            chain_api::chain_get_randomness::<DB, T>,
        )
        .with_method(
            "Filecoin.ChainGetBlock",
            chain_api::chain_get_block::<DB, T>,
        )
        .with_method("Filecoin.ChainHead", chain_api::chain_head::<DB, T>)
        .with_method(
            "Filecoin.WalletDefault",
            wallet_api::wallet_get_default::<DB, T>,
        )
        .with_method(
            "Filecoin.WalletListAddresses",
            wallet_api::wallet_list_addrs::<DB, T>,
        )
        .with_method("Filecoin.WalletHasKey", wallet_api::wallet_has_key::<DB, T>)
        .with_method("Filecoin.WalletNew", wallet_api::wallet_new::<DB, T>)
        .with_method(
            "Filecoin.WalletGetBalance",
            wallet_api::wallet_get_balance::<DB, T>,
        )
        .with_method("Filecoin.WalletExport", wallet_api::wallet_export::<DB, T>)
        .with_method("Filecoin.WalletImport", wallet_api::wallet_import::<DB, T>)
        .with_method("Filecoin.WalletSign", wallet_api::wallet_sign::<DB, T>)
        .with_method(
            "Filecoin.WalletSignMessage",
            wallet_api::wallet_sign_message::<DB, T>,
        )
        .with_method("Filecoin.WalletVerify", wallet_api::wallet_verify::<DB, T>)
        .with_method(
            "Filecoin.WalletSetDefault",
            wallet_api::wallet_set_default::<DB, T>,
        )
        .with_method("Filecoin.WalletDelete", wallet_api::wallet_delete::<DB, T>)
        .finish_unwrapped();
    let mut app = tide::Server::with_state(rpc);
    app.at("/api").post(handle_json_rpc);
    app.listen("127.0.0.1:8080").await.unwrap();
}
