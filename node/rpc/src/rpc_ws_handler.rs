// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use async_std::sync::Arc;
use crossbeam::atomic::AtomicCell;
use futures::StreamExt;
use log::{debug, error, info, warn};
use tide::http::headers::HeaderValues;
use tide_websockets::{Message, WebSocketConnection};

use beacon::Beacon;
use blockstore::BlockStore;
use chain::headchange_json::SubscriptionHeadChange;
use rpc_api::{
    chain_api::*,
    data_types::{JsonRpcServerState, StreamingData},
};

use crate::rpc_util::{call_rpc, call_rpc_str, check_permissions, get_auth_header, get_error_str};

async fn rpc_ws_task<DB, B>(
    authorization_header: Option<HeaderValues>,
    rpc_call: jsonrpc_v2::RequestObject,
    rpc_server: JsonRpcServerState,
    is_socket_active: Arc<AtomicCell<bool>>,
    ws_sender: WebSocketConnection,
) -> Result<(), tide::Error>
where
    DB: BlockStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
{
    let call_method = rpc_call.method_ref();
    let call_id = rpc_call.id_ref();

    check_permissions::<DB, B>(rpc_server.clone(), call_method, authorization_header).await?;

    match call_method {
        CHAIN_NOTIFY => {
            let request_id = match call_id {
                Some(id) => id.to_owned(),
                None => jsonrpc_v2::Id::Null,
            };

            debug!("Received ChainNotify request with RPC ID: {:?}", request_id);

            let (subscription_response, subscription_id) = call_rpc::<i64>(
                rpc_server.clone(),
                jsonrpc_v2::RequestObject::request()
                    .with_method(CHAIN_HEAD_SUBSCRIPTION)
                    .with_id(request_id.clone())
                    .finish(),
            )
            .await?;

            debug!(
                "Called ChainNotify RPC, got subscription ID {}",
                subscription_id
            );

            ws_sender.send(Message::Text(subscription_response)).await?;

            info!(
                "RPC WS ChainNotify for subscription ID: {}",
                subscription_id
            );

            while is_socket_active.load() {
                let (_, event) = call_rpc::<SubscriptionHeadChange>(
                    rpc_server.clone(),
                    jsonrpc_v2::RequestObject::request()
                        .with_method(CHAIN_NOTIFY)
                        .with_id(subscription_id)
                        .finish(),
                )
                .await?;

                debug!("Sending RPC WS ChainNotify event response");

                let event_response = StreamingData {
                    json_rpc: "2.0",
                    method: "xrpc.ch.val",
                    params: event,
                };

                match ws_sender
                    .send(Message::Text(serde_json::to_string(&event_response)?))
                    .await
                {
                    Ok(_) => {
                        info!(
                            "New ChainNotify data sent via subscription ID: {}",
                            subscription_id
                        );
                    }
                    Err(msg) => {
                        warn!("WS connection closed. {:?}", msg);
                        is_socket_active.store(false);
                    }
                }
            }
        }
        _ => {
            info!("RPC WS called method: {}", call_method);
            let response = call_rpc_str(rpc_server.clone(), rpc_call).await?;
            ws_sender.send(Message::Text(response)).await?;
        }
    }

    Ok(())
}

pub async fn rpc_ws_handler<DB, B>(
    request: tide::Request<JsonRpcServerState>,
    mut ws_stream: WebSocketConnection,
) -> Result<(), tide::Error>
where
    DB: BlockStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
{
    let (authorization_header, request) = get_auth_header(request);
    let rpc_server = request.state();
    let socket_active = Arc::new(AtomicCell::new(true));
    let ws_sender = ws_stream.clone();

    info!("Accepted WS connection!");

    while let Some(message_result) = ws_stream.next().await {
        debug!("Received new WS RPC message: {:?}", message_result);

        match message_result {
            Ok(message) => {
                let request_text = message.into_text()?;

                debug!("WS RPC Request: {}", request_text);

                if !request_text.is_empty() {
                    info!("RPC Request Received: {:?}", &request_text);

                    let authorization_header = authorization_header.clone();
                    let task_rpc_server = rpc_server.clone();
                    let task_socket_active = socket_active.clone();
                    let task_ws_sender = ws_sender.clone();

                    match serde_json::from_str(&request_text)
                        as Result<jsonrpc_v2::RequestObject, serde_json::Error>
                    {
                        Ok(rpc_call) => {
                            async_std::task::spawn(async move {
                                match rpc_ws_task::<DB, B>(
                                    authorization_header,
                                    rpc_call,
                                    task_rpc_server,
                                    task_socket_active,
                                    task_ws_sender.clone(),
                                )
                                .await
                                {
                                    Ok(_) => {
                                        debug!("WS RPC task success.");
                                    }
                                    Err(e) => {
                                        let msg = format!("WS RPC task error: {}", e);
                                        error!("{}", msg);
                                        task_ws_sender
                                            .send(Message::Text(get_error_str(3, msg)))
                                            .await
                                            .unwrap();
                                    }
                                }
                            });
                        }
                        Err(e) => {
                            let msg = format!("Error deserializing WS request payload: {}", e);
                            error!("{}", msg);
                            task_ws_sender
                                .send(Message::Text(get_error_str(1, msg)))
                                .await?;
                        }
                    }
                }
            }
            Err(e) => {
                let msg = format!(
                    "Error in WS socket stream. (Client possibly disconnected): {}",
                    e
                );
                error!("{}", msg);
                ws_sender.send(Message::Text(get_error_str(2, msg))).await?;
            }
        }
    }

    Ok(())
}
