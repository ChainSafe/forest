// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc_util::{call_rpc_str, check_permissions, get_auth_header, get_error_str};
use axum::{
    extract::{
        ws::{Message, WebSocket},
        WebSocketUpgrade,
    },
    response::IntoResponse,
};
use crossbeam::atomic::AtomicCell;
use forest_beacon::Beacon;
use forest_rpc_api::data_types::JsonRpcServerState;
use futures::{stream::SplitSink, SinkExt, StreamExt};
use fvm_ipld_blockstore::Blockstore;
use http::{HeaderMap, HeaderValue};
use log::{debug, error, info, warn};
use std::sync::Arc;
use tokio::sync::RwLock;

async fn rpc_ws_task<DB, B>(
    authorization_header: Option<HeaderValue>,
    rpc_call: jsonrpc_v2::RequestObject,
    rpc_server: JsonRpcServerState,
    _is_socket_active: Arc<AtomicCell<bool>>,
    ws_sender: Arc<RwLock<SplitSink<WebSocket, Message>>>,
) -> anyhow::Result<()>
where
    DB: Blockstore,
    B: Beacon,
{
    let call_method = rpc_call.method_ref();
    let _call_id = rpc_call.id_ref();

    check_permissions::<DB, B>(rpc_server.clone(), call_method, authorization_header)
        .await
        .map_err(|(_, e)| anyhow::Error::msg(e))?;

    info!("RPC WS called method: {}", call_method);
    let response = call_rpc_str(rpc_server.clone(), rpc_call).await?;
    ws_sender
        .write()
        .await
        .send(Message::Text(response))
        .await?;

    Ok(())
}

pub async fn rpc_ws_handler<DB, B>(
    headers: HeaderMap,
    axum::extract::State(rpc_server): axum::extract::State<JsonRpcServerState>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse
where
    DB: Blockstore,
    B: Beacon,
{
    let authorization_header = get_auth_header(headers);
    ws.on_upgrade(move |socket| async {
        rpc_ws_handler_inner::<DB, B>(socket, authorization_header, rpc_server).await
    })
}

async fn rpc_ws_handler_inner<DB, B>(
    socket: WebSocket,
    authorization_header: Option<HeaderValue>,
    rpc_server: JsonRpcServerState,
) where
    DB: Blockstore,
    B: Beacon,
{
    info!("Accepted WS connection!");
    let (sender, mut receiver) = socket.split();
    let ws_sender = Arc::new(RwLock::new(sender));
    let socket_active = Arc::new(AtomicCell::new(true));
    while let Some(Ok(message)) = receiver.next().await {
        debug!("Received new WS RPC message: {:?}", message);
        if let Message::Text(request_text) = message {
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
                        tokio::task::spawn(async move {
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
                                        .write()
                                        .await
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
                        if let Err(e) = task_ws_sender
                            .write()
                            .await
                            .send(Message::Text(get_error_str(1, msg)))
                            .await
                        {
                            warn!("{e}");
                        }
                    }
                }
            }
        }
    }
    socket_active.store(false);
}
