// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use async_std::sync::{Arc, Mutex};
use crossbeam::atomic::AtomicCell;
use futures::{SinkExt, StreamExt};
use log::{error, info, warn};
use tide_websockets::{Message, WebSocketConnection};

use beacon::Beacon;
use blockstore::BlockStore;
use wallet::KeyStore;

use crate::chain_api::Subscription;
use crate::data_types::{JsonRpcServerState, StreamingData};
use crate::rpc_util::{
    get_error_str, get_rpc_call_response, get_rpc_call_result, RPC_METHOD_CHAIN_HEAD_SUB,
    RPC_METHOD_CHAIN_NOTIFY,
};
use chain::headchange_json::HeadChangeJson;

pub async fn rpc_ws_handler<DB, KS, B>(
    request: tide::Request<JsonRpcServerState>,
    ws_stream: WebSocketConnection,
) -> Result<(), tide::Error>
where
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
{
    let rpc_server = request.state();
    let (ws_sender, mut ws_receiver) = ws_stream.split();
    let ws_sender = Arc::new(Mutex::new(ws_sender));
    let socket_active = Arc::new(AtomicCell::new(true));

    info!("Accepted WS connection!");

    while let Some(message_result) = ws_receiver.next().await {
        match message_result {
            Ok(message) => {
                let request_text = message.into_text()?;

                if !request_text.is_empty() {
                    info!("RPC Request Received: {:?}", &request_text);

                    match serde_json::from_str(&request_text)
                        as Result<jsonrpc_v2::RequestObject, serde_json::Error>
                    {
                        Ok(call) => match &*call.method_ref() {
                            RPC_METHOD_CHAIN_NOTIFY => {
                                let Subscription { subscription_id } = get_rpc_call_result(
                                    rpc_server.clone(),
                                    jsonrpc_v2::RequestObject::request()
                                        .with_method(RPC_METHOD_CHAIN_HEAD_SUB)
                                        .finish(),
                                )
                                .await?;

                                info!(
                                    "RPC WS ChainNotify for subscription ID: {}",
                                    subscription_id
                                );

                                let handler_rpc_server = rpc_server.clone();
                                let handler_socket_active = socket_active.clone();
                                let handler_ws_sender = ws_sender.clone();

                                async_std::task::spawn(async move {
                                    while handler_socket_active.load() {
                                        if let Some(event) =
                                            get_rpc_call_result::<Option<HeadChangeJson>>(
                                                handler_rpc_server.clone(),
                                                jsonrpc_v2::RequestObject::request()
                                                    .with_method(RPC_METHOD_CHAIN_NOTIFY)
                                                    .with_id(jsonrpc_v2::Id::Num(subscription_id))
                                                    .finish(),
                                            )
                                            .await
                                            .unwrap()
                                        {
                                            let response = StreamingData {
                                                json_rpc: "2.0",
                                                method: "xrpc.ch.val",
                                                params: (subscription_id, vec![event]),
                                            };

                                            match handler_ws_sender
                                                .lock()
                                                .await
                                                .send(Message::Text(
                                                    serde_json::to_string(&response).unwrap(),
                                                ))
                                                .await
                                            {
                                                Ok(_) => {
                                                    info!("New WS data sent.");
                                                }
                                                Err(msg) => {
                                                    warn!("WS connection closed. {:?}", msg);
                                                    handler_socket_active.store(false);
                                                }
                                            }
                                        }
                                    }
                                })
                                .await;
                            }
                            _ => {
                                info!("RPC WS called method: {}", call.method_ref());

                                let response =
                                    get_rpc_call_response(rpc_server.clone(), call).await?;

                                ws_sender.lock().await.send(Message::Text(response)).await?;
                            }
                        },
                        Err(e) => {
                            error!("Error deserializing WS request payload.");
                            ws_sender
                                .lock()
                                .await
                                .send(Message::Text(get_error_str(1, e.to_string())))
                                .await?;
                        }
                    }
                }
            }
            Err(e) => {
                error!("Error in WS socket stream. (Client possibly disconnected)");
                ws_sender
                    .lock()
                    .await
                    .send(Message::Text(get_error_str(2, e.to_string())))
                    .await?;
            }
        }
    }

    Ok(())
}
