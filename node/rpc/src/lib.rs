// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod chain_api;
mod gas_api;
mod mpool_api;
mod state_api;
mod sync_api;
mod wallet_api;

use crate::state_api::*;
use async_log::span;
use async_std::net::{TcpListener, TcpStream};
use async_std::sync::{RwLock, Sender};
use async_std::task;
use async_tungstenite::tungstenite::{error::Error, Message};
use blockstore::BlockStore;
use chain::ChainStore;
use chain_sync::{BadBlockCache, SyncState};
use forest_libp2p::NetworkMessage;
use futures::future;
use futures::sink::SinkExt;
use futures::stream::{StreamExt, TryStreamExt};
use jsonrpc_v2::{Data, MapRouter, RequestObject, Server};
use log::{error, info};
use message_pool::{MessagePool, MpoolRpcProvider};
use state_manager::StateManager;
use std::borrow::Cow;
use std::sync::Arc;
use wallet::KeyStore;
/// This is where you store persistant data, or at least access to stateful data.
pub struct RpcState<DB, KS>
where
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
{
    pub state_manager: StateManager<DB>,
    pub chain_store: Arc<RwLock<ChainStore<DB>>>,
    pub keystore: Arc<RwLock<KS>>,
    pub mpool: Arc<MessagePool<MpoolRpcProvider<DB>>>,
    pub bad_blocks: Arc<BadBlockCache>,
    pub sync_state: Arc<RwLock<SyncState>>,
    pub network_send: Sender<NetworkMessage>,
    pub network_name: String,
}

pub async fn start_rpc<DB, KS>(state: RpcState<DB, KS>, rpc_endpoint: &str)
where
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
{
    use chain_api::*;
    use gas_api::*;
    use mpool_api::*;
    use sync_api::*;
    use wallet_api::*;

    let rpc = Server::new()
        .with_data(Data::new(state))
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
        .with_method("Filecoin.ChainNotify", chain_notify::<DB, KS>)
        .with_method("Filecoin.ChainHead", chain_head::<DB, KS>)
        // Message Pool API
        .with_method(
            "Filecoin.MpoolEstimateGasPrice",
            estimate_gas_premium::<DB, KS>,
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
        // State API
        .with_method("Filecoin.StateMinerSector", state_miner_sector::<DB, KS>)
        .with_method("Filecoin.StateCall", state_call::<DB, KS>)
        .with_method(
            "Filecoin.StateMinerDeadlines",
            state_miner_deadlines::<DB, KS>,
        )
        .with_method(
            "Filecoin.StateSectorPrecommitInfo",
            state_sector_precommit_info::<DB, KS>,
        )
        .with_method("Filecoin.StateSectorInfo", state_sector_info::<DB, KS>)
        .with_method(
            "Filecoin.StateMinerProvingSet",
            state_miner_proving_set::<DB, KS>,
        )
        .with_method(
            "Filecoin.StateMinerProvingDeadline",
            state_miner_proving_deadline::<DB, KS>,
        )
        .with_method("Filecoin.StateMinerInfo", state_miner_info::<DB, KS>)
        .with_method("Filecoin.StateMinerFaults", state_miner_faults::<DB, KS>)
        .with_method(
            "Filecoin.StateAllMinerFaults",
            state_all_miner_faults::<DB, KS>,
        )
        .with_method(
            "Filecoin.StateMinerRecoveries",
            state_miner_recoveries::<DB, KS>,
        )
        .with_method("Filecoin.StateReplay", state_replay::<DB, KS>)
        .with_method("Filecoin.StateGetActor", state_get_actor::<DB, KS>)
        .with_method("Filecoin.StateAccountKey", state_account_key::<DB, KS>)
        .with_method("Filecoin.StateLookupId", state_lookup_id::<DB, KS>)
        .with_method(
            "Filecoin.StateMartketBalance",
            state_market_balance::<DB, KS>,
        )
        .with_method("Filecoin.StateGetReceipt", state_get_receipt::<DB, KS>)
        .with_method("Filecoin.StateWaitMsg", state_wait_msg::<DB, KS>)
        // Gas API
        .with_method(
            "Filecoin.GasEstimateGasLimit",
            gas_estimate_gas_limit::<DB, KS>,
        )
        .with_method(
            "Filecoin.GasEstimateGasPremium",
            gas_estimate_gas_premium::<DB, KS>,
        )
        .with_method("Filecoin.GasEstimateFeeCap", gas_estimate_fee_cap::<DB, KS>)
        .finish_unwrapped();

    let try_socket = TcpListener::bind(rpc_endpoint).await;
    let listener = try_socket.expect("Failed to bind to addr");
    let state = Arc::new(rpc);
    info!("waiting for web socket connections");
    while let Ok((stream, addr)) = listener.accept().await {
        task::spawn(handle_connection_and_log(state.clone(), stream, addr));
    }

    info!("Stopped accepting websocket connections");
}

async fn handle_connection_and_log(
    state: Arc<Server<MapRouter>>,
    tcp_stream: TcpStream,
    addr: std::net::SocketAddr,
) {
    span!("handle_connection", {
        if let Ok(ws_stream) = async_tungstenite::accept_async(tcp_stream).await {
            info!("accepted websocket connection at {:}", addr);
            let (mut ws_sender, ws_receiver) = ws_stream.split();
            let responses_result = ws_receiver
                .try_filter(|s| future::ready(!s.is_text()))
                .try_fold(Vec::new(), |mut responses, s| async {
                    let request_text = s.into_text()?;
                    let call: RequestObject = serde_json::from_str(&request_text)
                        .map_err(|s| Error::Protocol(Cow::from(s.to_string())))?;
                    let response = state.handle(call).await;
                    let response_text = serde_json::to_string(&response)
                        .map_err(|s| Error::Protocol(Cow::from(s.to_string())))?;
                    responses.push(response_text);
                    Ok(responses)
                })
                .await;

            if let Err(error) = responses_result {
                error!("error obtaining request {:?}", error)
            } else {
                for response_text in responses_result.unwrap() {
                    ws_sender
                        .send(Message::text(response_text))
                        .await
                        .unwrap_or_else(|s| error!("Error sending response {:?}", s))
                }
            }
        } else {
            error!("web socket connection failed at {:}", addr)
        }
    })
}
