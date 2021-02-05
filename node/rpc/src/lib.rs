// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod auth_api;
mod beacon_api;
mod chain_api;
mod common_api;
mod gas_api;
mod mpool_api;
mod net_api;
mod state_api;
mod sync_api;
mod wallet_api;

use crate::{beacon_api::beacon_get_entry, common_api::version, state_api::*};
use ahash::AHashMap;
use async_log::span;
use async_std::channel::{Receiver, Sender};
use async_std::net::{TcpListener, TcpStream};
use async_std::sync::{Arc, RwLock};
use async_std::task::{self};
use async_tungstenite::{
    tungstenite::handshake::server::Request, tungstenite::Message, WebSocketStream,
};
use auth::{has_perms, Error as AuthError, JWT_IDENTIFIER, WRITE_ACCESS};
use beacon::{Beacon, BeaconSchedule};
use blocks::Tipset;
use blockstore::BlockStore;
use chain::ChainStore;
use chain::{headchange_json::HeadChangeJson, HeadChange};
use chain_sync::{BadBlockCache, SyncState};
use fil_types::verifier::ProofVerifier;
use forest_libp2p::NetworkMessage;
use futures::sink::SinkExt;
use futures::stream::{SplitSink, StreamExt};
use jsonrpc_v2::{
    Data, Error, Id, MapRouter, RequestObject, ResponseObject, ResponseObjects, Server, V2,
};
use log::{debug, info, warn};
use message_pool::{MessagePool, MpoolRpcProvider};
use serde::Serialize;
use state_manager::StateManager;
use wallet::KeyStore;

type WsSink = SplitSink<WebSocketStream<TcpStream>, async_tungstenite::tungstenite::Message>;

const CHAIN_NOTIFY_METHOD_NAME: &str = "Filecoin.ChainNotify";
#[derive(Serialize)]
struct StreamingData<'a> {
    json_rpc: &'a str,
    method: &'a str,
    params: (usize, Vec<HeadChangeJson<'a>>),
}

/// This is where you store persistant data, or at least access to stateful data.
pub struct RpcState<DB, KS, B>
where
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
{
    pub state_manager: Arc<StateManager<DB>>,
    pub keystore: Arc<RwLock<KS>>,
    pub mpool: Arc<MessagePool<MpoolRpcProvider<DB>>>,
    pub bad_blocks: Arc<BadBlockCache>,
    pub sync_state: Arc<RwLock<Vec<Arc<RwLock<SyncState>>>>>,
    pub network_send: Sender<NetworkMessage>,
    pub new_mined_block_tx: Sender<Arc<Tipset>>,
    pub network_name: String,
    pub chain_store: Arc<ChainStore<DB>>,
    pub beacon: Arc<BeaconSchedule<B>>,
    // TODO in future, these should try to be removed, it currently isn't possible to handle
    // streaming with the current RPC framework. Should be able to just use subscribed channel.
    pub chain_notify_streams: AHashMap<usize, Receiver<HeadChange>>,
}

pub async fn start_rpc<DB, KS, B, V>(state: RpcState<DB, KS, B>, rpc_endpoint: &str)
where
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
    V: ProofVerifier + Send + Sync + 'static,
{
    use auth_api::*;
    use chain_api::*;
    use gas_api::*;
    use mpool_api::*;
    use sync_api::*;
    use wallet_api::*;

    let ks = state.keystore.clone();
    let cs = state.chain_store.clone();
    let rpc = Server::new()
        .with_data(Data::new(state))
        // Auth API
        .with_method("Filecoin.AuthNew", auth_new::<DB, KS, B>, false)
        .with_method("Filecoin.AuthVerify", auth_verify::<DB, KS, B>, false)
        // Chain API
        .with_method(
            "Filecoin.ChainGetMessage",
            chain_api::chain_get_message::<DB, KS, B>,
            false,
        )
        .with_method("Filecoin.ChainReadObj", chain_read_obj::<DB, KS, B>, false)
        .with_method("Filecoin.ChainHasObj", chain_has_obj::<DB, KS, B>, false)
        .with_method(
            "Filecoin.ChainGetBlockMessages",
            chain_block_messages::<DB, KS, B>,
            false,
        )
        .with_method(
            "Filecoin.ChainGetTipsetByHeight",
            chain_get_tipset_by_height::<DB, KS, B>,
            false,
        )
        .with_method(
            "Filecoin.ChainGetGenesis",
            chain_get_genesis::<DB, KS, B>,
            false,
        )
        .with_method(
            "Filecoin.ChainTipSetWeight",
            chain_tipset_weight::<DB, KS, B>,
            false,
        )
        .with_method(
            "Filecoin.ChainGetTipSet",
            chain_get_tipset::<DB, KS, B>,
            false,
        )
        .with_method(
            "Filecoin.ChainGetRandomnessFromTickets",
            chain_get_randomness_from_tickets::<DB, KS, B>,
            false,
        )
        .with_method(
            "Filecoin.ChainGetRandomnessFromBeacon",
            chain_get_randomness_from_beacon::<DB, KS, B>,
            false,
        )
        .with_method(
            "Filecoin.ChainGetBlock",
            chain_api::chain_get_block::<DB, KS, B>,
            false,
        )
        // * Filecoin.ChainNotify is handled specifically in middleware for streaming
        .with_method("Filecoin.ChainHead", chain_head::<DB, KS, B>, false)
        // Message Pool API
        .with_method(
            "Filecoin.MpoolEstimateGasPrice",
            estimate_gas_premium::<DB, KS, B>,
            false,
        )
        .with_method(
            "Filecoin.MpoolGetNonce",
            mpool_get_sequence::<DB, KS, B>,
            false,
        )
        .with_method("Filecoin.MpoolPending", mpool_pending::<DB, KS, B>, false)
        .with_method("Filecoin.MpoolPush", mpool_push::<DB, KS, B>, false)
        .with_method(
            "Filecoin.MpoolPushMessage",
            mpool_push_message::<DB, KS, B, V>,
            false,
        )
        .with_method("Filecoin.MpoolSelect", mpool_select::<DB, KS, B>, false)
        // Sync API
        .with_method("Filecoin.SyncCheckBad", sync_check_bad::<DB, KS, B>, false)
        .with_method("Filecoin.SyncMarkBad", sync_mark_bad::<DB, KS, B>, false)
        .with_method("Filecoin.SyncState", sync_state::<DB, KS, B>, false)
        .with_method(
            "Filecoin.SyncSubmitBlock",
            sync_submit_block::<DB, KS, B>,
            false,
        )
        // Wallet API
        .with_method("Filecoin.WalletBalance", wallet_balance::<DB, KS, B>, false)
        .with_method(
            "Filecoin.WalletDefaultAddress",
            wallet_default_address::<DB, KS, B>,
            false,
        )
        .with_method("Filecoin.WalletExport", wallet_export::<DB, KS, B>, false)
        .with_method("Filecoin.WalletHas", wallet_has::<DB, KS, B>, false)
        .with_method("Filecoin.WalletImport", wallet_import::<DB, KS, B>, false)
        .with_method("Filecoin.WalletList", wallet_list::<DB, KS, B>, false)
        .with_method("Filecoin.WalletNew", wallet_new::<DB, KS, B>, false)
        .with_method(
            "Filecoin.WalletSetDefault",
            wallet_set_default::<DB, KS, B>,
            false,
        )
        .with_method("Filecoin.WalletSign", wallet_sign::<DB, KS, B>, false)
        .with_method(
            "Filecoin.WalletSignMessage",
            wallet_sign_message::<DB, KS, B>,
            false,
        )
        .with_method("Filecoin.WalletVerify", wallet_verify::<DB, KS, B>, false)
        // State API
        .with_method(
            "Filecoin.StateMinerSectors",
            state_miner_sectors::<DB, KS, B>,
            false,
        )
        .with_method("Filecoin.StateCall", state_call::<DB, KS, B>, false)
        .with_method(
            "Filecoin.StateMinerDeadlines",
            state_miner_deadlines::<DB, KS, B>,
            false,
        )
        .with_method(
            "Filecoin.StateSectorPrecommitInfo",
            state_sector_precommit_info::<DB, KS, B>,
            false,
        )
        .with_method(
            "Filecoin.StateSectorGetInfo",
            state_sector_info::<DB, KS, B>,
            false,
        )
        .with_method(
            "Filecoin.StateMinerProvingDeadline",
            state_miner_proving_deadline::<DB, KS, B>,
            false,
        )
        .with_method(
            "Filecoin.StateMinerInfo",
            state_miner_info::<DB, KS, B>,
            false,
        )
        .with_method(
            "Filecoin.StateMinerFaults",
            state_miner_faults::<DB, KS, B>,
            false,
        )
        .with_method(
            "Filecoin.StateAllMinerFaults",
            state_all_miner_faults::<DB, KS, B>,
            false,
        )
        .with_method(
            "Filecoin.StateMinerRecoveries",
            state_miner_recoveries::<DB, KS, B>,
            false,
        )
        .with_method(
            "Filecoin.StateMinerPartitions",
            state_miner_partitions::<DB, KS, B>,
            false,
        )
        .with_method(
            "Filecoin.StateMinerPreCommitDepositForPower",
            state_miner_pre_commit_deposit_for_power::<DB, KS, B, V>,
            false,
        )
        .with_method(
            "Filecoin.StateMinerInitialPledgeCollateral",
            state_miner_initial_pledge_collateral::<DB, KS, B, V>,
            false,
        )
        .with_method("Filecoin.StateReplay", state_replay::<DB, KS, B>, false)
        .with_method(
            "Filecoin.StateGetActor",
            state_get_actor::<DB, KS, B, V>,
            false,
        )
        .with_method(
            "Filecoin.StateAccountKey",
            state_account_key::<DB, KS, B, V>,
            false,
        )
        .with_method(
            "Filecoin.StateLookupId",
            state_lookup_id::<DB, KS, B, V>,
            false,
        )
        .with_method(
            "Filecoin.StateMarketBalance",
            state_market_balance::<DB, KS, B>,
            false,
        )
        .with_method(
            "Filecoin.StateMarketDeals",
            state_market_deals::<DB, KS, B>,
            false,
        )
        .with_method(
            "Filecoin.StateGetReceipt",
            state_get_receipt::<DB, KS, B>,
            false,
        )
        .with_method("Filecoin.StateWaitMsg", state_wait_msg::<DB, KS, B>, false)
        .with_method(
            "Filecoin.StateMinerSectorAllocated",
            state_miner_sector_allocated::<DB, KS, B>,
            false,
        )
        .with_method(
            "Filecoin.StateNetworkName",
            state_network_name::<DB, KS, B>,
            false,
        )
        .with_method(
            "Filecoin.MinerGetBaseInfo",
            state_miner_get_base_info::<DB, KS, B, V>,
            false,
        )
        .with_method(
            "Filecoin.MinerCreateBlock",
            miner_create_block::<DB, KS, B, V>,
            false,
        )
        .with_method(
            "Filecoin.StateNetworkVersion",
            state_get_network_version::<DB, KS, B>,
            false,
        )
        // Gas API
        .with_method(
            "Filecoin.GasEstimateGasLimit",
            gas_estimate_gas_limit::<DB, KS, B, V>,
            false,
        )
        .with_method(
            "Filecoin.GasEstimateGasPremium",
            gas_estimate_gas_premium::<DB, KS, B>,
            false,
        )
        .with_method(
            "Filecoin.GasEstimateFeeCap",
            gas_estimate_fee_cap::<DB, KS, B>,
            false,
        )
        .with_method(
            "Filecoin.GasEstimateMessageGas",
            gas_estimate_message_gas::<DB, KS, B, V>,
            false,
        )
        // Common
        .with_method("Filecoin.Version", version, false)
        //beacon
        .with_method(
            "Filecoin.BeaconGetEntry",
            beacon_get_entry::<DB, KS, B>,
            false,
        )
        // Net
        .with_method(
            "Filecoin.NetAddrsListen",
            net_api::net_addrs_listen::<DB, KS, B>,
            false,
        )
        .finish_unwrapped();

    let try_socket = TcpListener::bind(rpc_endpoint).await;
    let listener = try_socket.expect("Failed to bind to addr");
    let rpc_state = Arc::new(rpc);
    let chain_notify_count: Arc<RwLock<usize>> = Default::default();

    info!("waiting for web socket connections");
    while let Ok((stream, addr)) = listener.accept().await {
        task::spawn(handle_connection_and_log(
            rpc_state.clone(),
            ks.clone(),
            cs.clone(),
            stream,
            addr,
            chain_notify_count.clone(),
        ));
    }

    info!("Stopped accepting websocket connections");
}

async fn handle_connection_and_log<KS, DB>(
    state: Arc<Server<MapRouter>>,
    ks: Arc<RwLock<KS>>,
    cs: Arc<ChainStore<DB>>,
    tcp_stream: TcpStream,
    addr: std::net::SocketAddr,
    chain_notify_count: Arc<RwLock<usize>>,
) where
    KS: KeyStore + Send + Sync + 'static,
    DB: BlockStore + Send + Sync + 'static,
{
    span!("handle_connection_and_log", {
        let mut authorization_header: Arc<Option<String>> = Arc::new(None);
        if let Ok(ws_stream) =
            async_tungstenite::accept_hdr_async(tcp_stream, |request: &Request, response| {
                if let Some(authorization) = request.headers().get("Authorization") {
                    // not all methods require authorization
                    authorization_header = Arc::new(
                        authorization
                            .to_str()
                            .map(|s| Some(s.to_string()))
                            .unwrap_or_default(),
                    );
                }
                Ok(response)
            })
            .await
        {
            debug!("accepted websocket connection at {:}", addr);
            let (ws_sender, mut ws_receiver) = ws_stream.split();
            let ws_sender = Arc::new(RwLock::new(ws_sender));
            while let Some(message_result) = ws_receiver.next().await {
                let s = state.clone();
                let ks_clone = ks.clone();
                let auth_header_clone = authorization_header.clone();
                let chain_notify_count_shared = chain_notify_count.clone();
                let ws_sender = Arc::clone(&ws_sender);
                let cs = Arc::clone(&cs);
                task::spawn(async move {
                    match message_result {
                        Ok(message) => {
                            let request_text = message.into_text().unwrap();
                            if request_text.is_empty() {
                                return;
                            }
                            info!("RPC Request Received: {:?}", request_text.clone());
                            match serde_json::from_str(&request_text)
                                as Result<RequestObject, serde_json::Error>
                            {
                                Ok(call) => {
                                    // hacky but due to the limitations of jsonrpc_v2 impl
                                    // if this expands, better to implement some sort of middleware

                                    if &*call.method == CHAIN_NOTIFY_METHOD_NAME {
                                        let mut x = chain_notify_count_shared.write().await;
                                        *x += 1;
                                        let chain_notify_count_curr = *x;
                                        drop(x);

                                        let mut head_changes = cs.sub_head_changes().await;

                                        // TODO remove this manually constructed RPC response
                                        #[derive(Serialize)]
                                        struct SubscribeChannelIDResponse<'a> {
                                            json_rpc: &'a str,
                                            result: usize,
                                            id: Id,
                                        }

                                        // First response should be the count serialized.
                                        // This is based on internal golang channel rpc handling
                                        // needed to match Lotus.
                                        let data = SubscribeChannelIDResponse {
                                            json_rpc: "2.0",
                                            result: chain_notify_count_curr,
                                            id: call.id.flatten().unwrap_or_default(),
                                        };
                                        if let Err(e) = send_response(&ws_sender, &data).await {
                                            let msg = e.message();
                                            send_error(3, &ws_sender, msg).await;
                                            return;
                                        }

                                        let ws_sender = ws_sender.clone();
                                        task::spawn(async move {
                                            while let Some(event) = head_changes.next().await {
                                                let data = StreamingData {
                                                    json_rpc: "2.0",
                                                    method: "xrpc.ch.val",
                                                    params: (
                                                        chain_notify_count_curr,
                                                        vec![HeadChangeJson::from(&event)],
                                                    ),
                                                };

                                                if let Err(e) =
                                                    send_response(&ws_sender, &data).await
                                                {
                                                    let msg = e.message();
                                                    send_error(3, &ws_sender, msg).await;
                                                }
                                            }

                                            Ok::<(), Error>(())
                                        });
                                    } else {
                                        // In cases of non-streaming, just write the response
                                        // to the socket
                                        handle_rpc(
                                            &ws_sender,
                                            &s.clone(),
                                            &ks_clone,
                                            call,
                                            &auth_header_clone.as_ref(),
                                        )
                                        .await;
                                    };
                                }
                                Err(e) => send_error(1, &ws_sender, e.to_string()).await,
                            }
                        }
                        Err(e) => send_error(2, &ws_sender, e.to_string()).await,
                    };
                });
            }
        } else {
            warn!("web socket connection failed at {:}", addr);
        }
    })
}

async fn handle_rpc<KS: KeyStore>(
    ws_sender: &RwLock<WsSink>,
    state: &Arc<Server<MapRouter>>,
    ks: &Arc<RwLock<KS>>,
    call: RequestObject,
    authorization_header: &Option<String>,
) {
    if WRITE_ACCESS.contains(&&*call.method) {
        if let Some(header) = authorization_header {
            match ks
                .read()
                .await
                .get(JWT_IDENTIFIER)
                .map_err(|_| AuthError::Other("No JWT private key found".to_owned()))
            {
                Ok(key) => {
                    if let Err(e) = has_perms(header.to_string(), "write", key.private_key()) {
                        let msg = e.message();
                        send_error(3, ws_sender, msg).await;
                        return;
                    }
                }
                Err(e) => {
                    send_error(3, ws_sender, e.to_string()).await;
                }
            }
        } else {
            send_error(200, ws_sender, AuthError::NoAuthHeader.to_string()).await;
        }
    };

    let response = state.handle(call).await;
    if let Err(e) = send_response(ws_sender, response).await {
        let msg = e.message();
        send_error(3, &ws_sender, msg).await;
    }
}

async fn send_response<R>(ws_sender: &RwLock<WsSink>, response: R) -> Result<(), Error>
where
    R: Serialize,
{
    let response_text = serde_json::to_string(&response)?;
    ws_sender
        .write()
        .await
        .send(Message::text(response_text))
        .await?;
    Ok(())
}

async fn send_error(code: i64, ws_sender: &RwLock<WsSink>, message: String) {
    let response = ResponseObjects::One(ResponseObject::Error {
        jsonrpc: V2,
        error: Error::Full {
            code,
            message,
            data: None,
        },
        id: Id::Null,
    });
    let serialized = serde_json::to_string(&response);
    match serialized {
        Ok(res) => {
            if let Err(e) = ws_sender.write().await.send(Message::text(res)).await {
                log::error!("failed to send websocket error: {}", e);
            }
        }
        Err(e) => {
            log::error!("failed to serialize websocket error: {}", e);
        }
    }
}
