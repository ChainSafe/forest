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
use async_std::sync::Arc;
use async_std::sync::{RwLock, Sender};
use async_std::task;
use async_tungstenite::{tungstenite::Message, WebSocketStream};
use blocks::Tipset;
use blockstore::BlockStore;
use chain::headchange_json::{HeadChangeJson, IndexToHeadChangeJson};
use chain::HeadChange;
use chain_sync::{BadBlockCache, SyncState};
use flo_stream::{MessagePublisher, Publisher, Subscriber};
use forest_libp2p::NetworkMessage;
use futures::sink::SinkExt;
use futures::stream::{SplitSink, StreamExt};
use jsonrpc_v2::{
    Data, Error, Id, MapRouter, RequestBuilder, RequestObject, ResponseObject, ResponseObjects,
    Server, V2,
};
use log::{info, warn};
use message_pool::{MessagePool, MpoolRpcProvider};
use serde::Serialize;
use state_manager::StateManager;
use wallet::KeyStore;

type WsSink = SplitSink<WebSocketStream<TcpStream>, async_tungstenite::tungstenite::Message>;

const CHAIN_NOTIFY_METHOD_NAME: &str = "Filecoin.ChainNotify";
#[derive(Serialize)]
struct StreamingData {
    json_rpc: String,
    method: String,
    params: (usize, Vec<HeadChangeJson>),
}

/// This is where you store persistant data, or at least access to stateful data.
pub struct RpcState<DB, KS>
where
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
{
    pub state_manager: Arc<StateManager<DB>>,
    pub keystore: Arc<RwLock<KS>>,
    pub heaviest_tipset: Arc<RwLock<Option<Arc<Tipset>>>>,
    pub subscriber: Subscriber<HeadChange>,
    pub publisher: Arc<RwLock<Publisher<IndexToHeadChangeJson>>>,
    pub mpool: Arc<MessagePool<MpoolRpcProvider<DB>>>,
    pub bad_blocks: Arc<BadBlockCache>,
    pub sync_state: Arc<RwLock<Vec<Arc<RwLock<SyncState>>>>>,
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
    let subscriber = state.publisher.write().await.subscribe();
    let rpc = Server::new()
        .with_data(Data::new(state))
        .with_method(
            "Filecoin.ChainGetMessage",
            chain_api::chain_get_message::<DB, KS>,
            false,
        )
        .with_method("Filecoin.ChainGetObj", chain_read_obj::<DB, KS>, false)
        .with_method("Filecoin.ChainHasObj", chain_has_obj::<DB, KS>, false)
        .with_method(
            "Filecoin.ChainGetBlockMessages",
            chain_block_messages::<DB, KS>,
            false,
        )
        .with_method(
            "Filecoin.ChainGetTipsetByHeight",
            chain_get_tipset_by_height::<DB, KS>,
            false,
        )
        .with_method(
            "Filecoin.ChainGetGenesis",
            chain_get_genesis::<DB, KS>,
            false,
        )
        .with_method(
            "Filecoin.ChainTipsetWeight",
            chain_tipset_weight::<DB, KS>,
            false,
        )
        .with_method("Filecoin.ChainGetTipset", chain_get_tipset::<DB, KS>, false)
        .with_method(
            "Filecoin.GetRandomness",
            chain_get_randomness::<DB, KS>,
            false,
        )
        .with_method(
            "Filecoin.ChainGetBlock",
            chain_api::chain_get_block::<DB, KS>,
            false,
        )
        .with_method(CHAIN_NOTIFY_METHOD_NAME, chain_notify::<DB, KS>, true)
        .with_method("Filecoin.ChainHead", chain_head::<DB, KS>, false)
        // Message Pool API
        .with_method(
            "Filecoin.MpoolEstimateGasPrice",
            estimate_gas_premium::<DB, KS>,
            false,
        )
        .with_method(
            "Filecoin.MpoolGetNonce",
            mpool_get_sequence::<DB, KS>,
            false,
        )
        .with_method("Filecoin.MpoolPending", mpool_pending::<DB, KS>, false)
        .with_method("Filecoin.MpoolPush", mpool_push::<DB, KS>, false)
        .with_method(
            "Filecoin.MpoolPushMessage",
            mpool_push_message::<DB, KS>,
            false,
        )
        // Sync API
        .with_method("Filecoin.SyncCheckBad", sync_check_bad::<DB, KS>, false)
        .with_method("Filecoin.SyncMarkBad", sync_mark_bad::<DB, KS>, false)
        .with_method("Filecoin.SyncState", sync_state::<DB, KS>, false)
        .with_method(
            "Filecoin.SyncSubmitBlock",
            sync_submit_block::<DB, KS>,
            false,
        )
        // Wallet API
        .with_method("Filecoin.WalletBalance", wallet_balance::<DB, KS>, false)
        .with_method(
            "Filecoin.WalletDefaultAddress",
            wallet_default_address::<DB, KS>,
            false,
        )
        .with_method("Filecoin.WalletExport", wallet_export::<DB, KS>, false)
        .with_method("Filecoin.WalletHas", wallet_has::<DB, KS>, false)
        .with_method("Filecoin.WalletImport", wallet_import::<DB, KS>, false)
        .with_method("Filecoin.WalletList", wallet_list::<DB, KS>, false)
        .with_method("Filecoin.WalletNew", wallet_new::<DB, KS>, false)
        .with_method(
            "Filecoin.WalletSetDefault",
            wallet_set_default::<DB, KS>,
            false,
        )
        .with_method("Filecoin.WalletSign", wallet_sign::<DB, KS>, false)
        .with_method(
            "Filecoin.WalletSignMessage",
            wallet_sign_message::<DB, KS>,
            false,
        )
        .with_method("Filecoin.WalletVerify", wallet_verify::<DB, KS>, false)
        // State API
        .with_method(
            "Filecoin.StateMinerSector",
            state_miner_sector::<DB, KS>,
            false,
        )
        .with_method("Filecoin.StateCall", state_call::<DB, KS>, false)
        .with_method(
            "Filecoin.StateMinerDeadlines",
            state_miner_deadlines::<DB, KS>,
            false,
        )
        .with_method(
            "Filecoin.StateSectorPrecommitInfo",
            state_sector_precommit_info::<DB, KS>,
            false,
        )
        .with_method(
            "Filecoin.StateSectorInfo",
            state_sector_info::<DB, KS>,
            false,
        )
        .with_method(
            "Filecoin.StateMinerProvingSet",
            state_miner_proving_set::<DB, KS>,
            false,
        )
        .with_method(
            "Filecoin.StateMinerProvingDeadline",
            state_miner_proving_deadline::<DB, KS>,
            false,
        )
        .with_method("Filecoin.StateMinerInfo", state_miner_info::<DB, KS>, false)
        .with_method(
            "Filecoin.StateMinerFaults",
            state_miner_faults::<DB, KS>,
            false,
        )
        .with_method(
            "Filecoin.StateAllMinerFaults",
            state_all_miner_faults::<DB, KS>,
            false,
        )
        .with_method(
            "Filecoin.StateMinerRecoveries",
            state_miner_recoveries::<DB, KS>,
            false,
        )
        .with_method("Filecoin.StateReplay", state_replay::<DB, KS>, false)
        .with_method("Filecoin.StateGetActor", state_get_actor::<DB, KS>, false)
        .with_method(
            "Filecoin.StateAccountKey",
            state_account_key::<DB, KS>,
            false,
        )
        .with_method("Filecoin.StateLookupId", state_lookup_id::<DB, KS>, false)
        .with_method(
            "Filecoin.StateMartketBalance",
            state_market_balance::<DB, KS>,
            false,
        )
        .with_method(
            "Filecoin.StateGetReceipt",
            state_get_receipt::<DB, KS>,
            false,
        )
        .with_method("Filecoin.StateWaitMsg", state_wait_msg::<DB, KS>, false)
        // Gas API
        .with_method(
            "Filecoin.GasEstimateGasLimit",
            gas_estimate_gas_limit::<DB, KS>,
            false,
        )
        .with_method(
            "Filecoin.GasEstimateGasPremium",
            gas_estimate_gas_premium::<DB, KS>,
            false,
        )
        .with_method(
            "Filecoin.GasEstimateFeeCap",
            gas_estimate_fee_cap::<DB, KS>,
            false,
        )
        .finish_unwrapped();

    let try_socket = TcpListener::bind(rpc_endpoint).await;
    let listener = try_socket.expect("Failed to bind to addr");
    let state = Arc::new(rpc);

    info!("waiting for web socket connections");
    while let Ok((stream, addr)) = listener.accept().await {
        task::spawn(handle_connection_and_log(
            state.clone(),
            stream,
            addr,
            subscriber.clone(),
        ));
    }

    info!("Stopped accepting websocket connections");
}

async fn handle_connection_and_log(
    state: Arc<Server<MapRouter>>,
    tcp_stream: TcpStream,
    addr: std::net::SocketAddr,
    subscriber: Subscriber<IndexToHeadChangeJson>,
) {
    span!("handle_connection_and_log", {
        if let Ok(ws_stream) = async_tungstenite::accept_async(tcp_stream).await {
            info!("accepted websocket connection at {:}", addr);
            let (ws_sender, mut ws_receiver) = ws_stream.split();
            let ws_sender = Arc::new(RwLock::new(ws_sender));
            let mut chain_notify_count = 0;
            while let Some(message_result) = ws_receiver.next().await {
                match message_result {
                    Ok(message) => {
                        let request_text = message.into_text().unwrap();
                        info!(
                            "serde request {:?}",
                            serde_json::to_string_pretty(&request_text).unwrap()
                        );
                        match serde_json::from_str(&request_text) as Result<RequestObject, _> {
                            Ok(call) => {
                                // hacky but due to the limitations of jsonrpc_v2 impl
                                // if this expands, better to implement some sort of middleware
                                let call = if &*call.method == CHAIN_NOTIFY_METHOD_NAME {
                                    chain_notify_count += 1;
                                    RequestBuilder::default()
                                        .with_id(call.id.unwrap_or_default().unwrap_or_default())
                                        .with_params(chain_notify_count)
                                        .with_method(CHAIN_NOTIFY_METHOD_NAME)
                                        .finish()
                                } else {
                                    call
                                };
                                let response = state.clone().handle(call).await;
                                streaming_payload_and_log(
                                    ws_sender.clone(),
                                    response,
                                    subscriber.clone(),
                                    chain_notify_count,
                                )
                                .await;
                            }
                            Err(_) => {
                                let response = ResponseObjects::One(ResponseObject::Error {
                                    jsonrpc: V2,
                                    error: Error::Provided {
                                        code: 1,
                                        message: "Error serialization request",
                                    },
                                    id: Id::Null,
                                });
                                streaming_payload_and_log(
                                    ws_sender.clone(),
                                    response,
                                    subscriber.clone(),
                                    chain_notify_count,
                                )
                                .await;
                            }
                        }
                    }
                    Err(_) => {
                        let response = ResponseObjects::One(ResponseObject::Error {
                            jsonrpc: V2,
                            error: Error::Provided {
                                code: 2,
                                message: "Error reading request",
                            },
                            id: Id::Null,
                        });
                        streaming_payload_and_log(
                            ws_sender.clone(),
                            response,
                            subscriber.clone(),
                            chain_notify_count,
                        )
                        .await;
                    }
                };
            }
        } else {
            warn!("web socket connection failed at {:}", addr)
        }
    })
}

async fn streaming_payload_and_log(
    ws_sender: Arc<RwLock<WsSink>>,
    response_object: ResponseObjects,
    mut subscriber: Subscriber<IndexToHeadChangeJson>,
    streaming_count: usize,
) {
    let response_text = serde_json::to_string(&response_object).unwrap();
    ws_sender
        .write()
        .await
        .send(Message::text(response_text))
        .await
        .unwrap_or_else(|_| warn!("Could not send to response to socket"));
    if let ResponseObjects::One(ResponseObject::Result {
        jsonrpc: _,
        result: _,
        id: _,
        streaming,
    }) = response_object
    {
        if streaming {
            task::spawn(async move {
                let sender = ws_sender.clone();
                while let Some(index_to_head_change) = subscriber.next().await {
                    if streaming_count == index_to_head_change.0 {
                        let data = StreamingData {
                            json_rpc: "2.0".to_string(),
                            method: "xrpc.ch.val".to_string(),
                            params: (streaming_count, vec![index_to_head_change.1]),
                        };
                        let response_text =
                            serde_json::to_string(&data).expect("Bad Serialization of Type");
                        sender
                            .write()
                            .await
                            .send(Message::text(response_text))
                            .await
                            .unwrap_or_else(|_| warn!("Could not send to response to socket"));
                    }
                }
            });
        }
    }
}
