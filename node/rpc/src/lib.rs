// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#[macro_use]
extern crate lazy_static;

mod auth_api;
mod chain_api;
mod mpool_api;
mod sync_api;
mod wallet_api;

use async_std::sync::{RwLock, Sender};
use auth::{has_perms, Error};
use blockstore::BlockStore;
use chain_sync::{BadBlockCache, SyncState};
use forest_libp2p::NetworkMessage;
use jsonrpc_v2::{Data, Error as JsonRpcError, ErrorLike, MapRouter, RequestObject, Server};
use message_pool::{MessagePool, MpoolRpcProvider};
use std::sync::Arc;
use tide::{Request, Response, StatusCode};
use utils::get_home_dir;
use wallet::KeyStore;
use wallet::PersistentKeyStore;

lazy_static! {
    pub static ref WRITE_ACCESS: Vec<String> = vec![
        "Filecoin.MpoolPush".to_string(),
        "Filecoin.WalletNew".to_string(),
        "Filecoin.WalletHas".to_string(),
        "Filecoin.WalletList".to_string(),
        "Filecoin.WalletDefaultAddress".to_string(),
        "Filecoin.WalletList".to_string(),
    ];
}

/// This is where you store persistant data, or at least access to stateful data.
pub struct RpcState<DB, KS>
where
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
{
    pub store: Arc<DB>,
    pub keystore: Arc<RwLock<KS>>,
    pub mpool: Arc<MessagePool<MpoolRpcProvider<DB>>>,
    pub bad_blocks: Arc<BadBlockCache>,
    pub sync_state: Arc<RwLock<SyncState>>,
    pub network_send: Sender<NetworkMessage>,
    pub network_name: String,
}

async fn handle_json_rpc(mut req: Request<Server<MapRouter>>) -> tide::Result {
    let call: RequestObject = req.body_json().await?;
    // TODO find a cleaner way *if possibe* to parse the RequestObject to get the method name in RPC call
    let call_str = format!("{:?}", call);
    let start = call_str
        .find("method: \"")
        .ok_or_else(|| Error::MethodParam)?
        + 9;
    let end = call_str
        .find("\", params")
        .ok_or_else(|| Error::MethodParam)?;
    let method_name = &call_str[start..end];
    // check for write access
    if WRITE_ACCESS.contains(&method_name.to_string()) {
        if let Some(header) = req.header("Authorization") {
            let header_raw = header.get(0).unwrap().message();
            let keystore = PersistentKeyStore::new(get_home_dir() + "/.forest")?;
            let ki = keystore
                .get("auth-jwt-private")
                .map_err(|_| Error::Other("No JWT private key found".to_owned()))?;
            let key = ki.private_key();
            let perm = has_perms(header_raw, "write", key);
            if perm.is_err() {
                return Ok(Response::new(StatusCode::Ok).body_json(&perm.unwrap_err())?);
            }
        } else {
            return Ok(Response::new(StatusCode::Ok)
                .body_json(&JsonRpcError::from(Error::NoAuthHeader))?);
        }
    }

    let res = req.state().handle(call).await;
    Ok(Response::new(StatusCode::Ok).body_json(&res)?)
}

pub async fn start_rpc<DB, KS>(state: RpcState<DB, KS>, rpc_endpoint: &str)
where
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
{
    use auth_api::*;
    use chain_api::*;
    use mpool_api::*;
    use sync_api::*;
    use wallet_api::*;

    let rpc = Server::new()
        .with_data(Data::new(state))
        // Auth API
        .with_method("Filecoin.AuthNew", auth_new::<DB, KS>)
        .with_method("Filecoin.AuthVerify", auth_verify::<DB, KS>)
        // Chain API
        .with_method(
            "Filecoin.ChainGetMessage",
            chain_api::chain_get_message::<DB, KS>,
        )
        .with_method("Filecoin.ChainGetObj", chain_read_obj::<DB, KS>)
        .with_method("Filecoin.ChainHasObj", chain_has_obj::<DB, KS>)
        .with_method(
            "Filecoin.ChainGetBlockMessages",
            chain_block_messages::<DB, KS>,
        )
        .with_method(
            "Filecoin.ChainGetTipsetByHeight",
            chain_get_tipset_by_height::<DB, KS>,
        )
        .with_method("Filecoin.ChainGetGenesis", chain_get_genesis::<DB, KS>)
        .with_method("Filecoin.ChainTipsetWeight", chain_tipset_weight::<DB, KS>)
        .with_method("Filecoin.ChainGetTipset", chain_get_tipset::<DB, KS>)
        .with_method("Filecoin.GetRandomness", chain_get_randomness::<DB, KS>)
        .with_method(
            "Filecoin.ChainGetBlock",
            chain_api::chain_get_block::<DB, KS>,
        )
        .with_method("Filecoin.ChainHead", chain_head::<DB, KS>)
        // Message Pool API
        .with_method(
            "Filecoin.MpoolEstimateGasPrice",
            mpool_estimate_gas_price::<DB, KS>,
        )
        .with_method("Filecoin.MpoolGetNonce", mpool_get_sequence::<DB, KS>)
        .with_method("Filecoin.MpoolPending", mpool_pending::<DB, KS>)
        .with_method("Filecoin.MpoolPush", mpool_push::<DB, KS>)
        .with_method("Filecoin.MpoolPushMessage", mpool_push_message::<DB, KS>)
        // Sync API
        .with_method("Filecoin.SyncCheckBad", sync_check_bad::<DB, KS>)
        .with_method("Filecoin.SyncMarkBad", sync_mark_bad::<DB, KS>)
        .with_method("Filecoin.SyncState", sync_state::<DB, KS>)
        .with_method("Filecoin.SyncSubmitBlock", sync_submit_block::<DB, KS>)
        // Wallet API
        .with_method("Filecoin.WalletBalance", wallet_balance::<DB, KS>)
        .with_method(
            "Filecoin.WalletDefaultAddress",
            wallet_default_address::<DB, KS>,
        )
        .with_method("Filecoin.WalletExport", wallet_export::<DB, KS>)
        .with_method("Filecoin.WalletHas", wallet_has::<DB, KS>)
        .with_method("Filecoin.WalletImport", wallet_import::<DB, KS>)
        .with_method("Filecoin.WalletList", wallet_list::<DB, KS>)
        .with_method("Filecoin.WalletNew", wallet_new::<DB, KS>)
        .with_method("Filecoin.WalletSetDefault", wallet_set_default::<DB, KS>)
        .with_method("Filecoin.WalletSign", wallet_sign::<DB, KS>)
        .with_method("Filecoin.WalletSignMessage", wallet_sign_message::<DB, KS>)
        .with_method("Filecoin.WalletVerify", wallet_verify::<DB, KS>)
        .finish_unwrapped();

    let mut app = tide::Server::with_state(rpc);
    app.at("/rpc/v0").post(handle_json_rpc);
    app.listen(rpc_endpoint).await.unwrap();
}
