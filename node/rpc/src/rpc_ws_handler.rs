use futures::stream::StreamExt;
use jsonrpc_v2::{
    Id as JsonRpcId, RequestObject as JsonRequestObject, ResponseObject as JsonResponseObject,
    ResponseObjects as JsonResponseObjects,
};
use log::{debug, error, info};
use serde::{Deserialize, Serialize};
use tide::Request;
use tide_websockets::WebSocketConnection;

use beacon::Beacon;
use blockstore::BlockStore;
use wallet::KeyStore;

use crate::chain_api::Subscription;
use crate::data_types::{JsonRpcServerState, StreamingData};
use crate::rpc_util::{get_error_str, RPC_METHOD_CHAIN_HEAD_SUB, RPC_METHOD_CHAIN_NOTIFY};
use chain::headchange_json::HeadChangeJson;
use rpc_types::JsonRpcRequestObject;

pub async fn rpc_ws_handler<DB, KS, B>(
    request: Request<JsonRpcServerState>,
    mut ws_stream: WebSocketConnection,
) -> Result<(), tide::Error>
where
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
{
    let rpc_server = request.state();

    debug!("accepted websocket connection from {:?}", request.remote());

    while let Some(message_result) = ws_stream.next().await {
        match message_result {
            Ok(message) => {
                let request_text = message.into_text()?;

                if request_text.is_empty() {
                    Ok(())
                } else {
                    info!("RPC Request Received: {:?}", &request_text);

                    match serde_json::from_str(&request_text)
                        as Result<JsonRpcRequestObject, serde_json::Error>
                    {
                        Ok(call) => {
                            match &*call.method {
                                RPC_METHOD_CHAIN_NOTIFY => {
                                    #[derive(Deserialize)]
                                    struct SubscribeChannelIDResponse<'a> {
                                        json_rpc: &'a str,
                                        result: usize,
                                        id: JsonRpcId,
                                    }

                                    let rpc_subscription_response = rpc_server
                                        .handle(
                                            JsonRequestObject::request()
                                                .with_method(RPC_METHOD_CHAIN_HEAD_SUB)
                                                .finish(),
                                        )
                                        .await;

                                    match rpc_subscription_response {
                                        JsonResponseObjects::One(rpc_subscription_params) => {
                                            match rpc_subscription_params {
                                                JsonResponseObject::Result { result, .. } => {
                                                    let subscription_response =
                                                        serde_json::from_value::<Subscription>(
                                                            serde_json::to_value(result)?,
                                                        )?;

                                                    let Subscription { subscription_id } =
                                                        subscription_response;

                                                    rpc_server
                                                        .handle(
                                                            JsonRequestObject::request()
                                                                .with_method(
                                                                    RPC_METHOD_CHAIN_NOTIFY,
                                                                )
                                                                .with_id(JsonRpcId::Num(
                                                                    subscription_id,
                                                                ))
                                                                .finish(),
                                                        )
                                                        .await;

                                                    let mut head_changes =
                                                        cs.sub_head_changes().await;

                                                    // First response should be the count serialized.
                                                    // This is based on internal golang channel rpc handling
                                                    // needed to match Lotus.
                                                    let response = SubscribeChannelIDResponse {
                                                        json_rpc: "2.0",
                                                        result: chain_notify_count.load(),
                                                        id: call
                                                            .id
                                                            .flatten()
                                                            .unwrap_or(JsonRpcId::Null),
                                                    };

                                                    ws_stream.send_json(&response).await?; // TODO: handle send error

                                                    while let Some(event) =
                                                        head_changes.next().await
                                                    {
                                                        let response = StreamingData {
                                                            json_rpc: "2.0",
                                                            method: "xrpc.ch.val",
                                                            params: (
                                                                chain_notify_count.load(),
                                                                vec![HeadChangeJson::from(&event)],
                                                            ),
                                                        };

                                                        ws_stream.send_json(&response).await?;
                                                    }

                                                    Ok(())
                                                }
                                                JsonResponseObject::Error(_) => Err(()),
                                            }
                                        }
                                        _ => Err(()),
                                    }
                                }
                                _ => {
                                    error!(
                                        "RPC Websocket tried handling something it shouldn't have."
                                    );

                                    // handle like http rpc
                                    let rpc_response = rpc_server
                                        .handle(
                                            JsonRequestObject::request()
                                                .with_method(call.method)
                                                .with_params(call.params)
                                                .with_id(call.id)
                                                .finish(),
                                        )
                                        .await;

                                    // TODO: ws response
                                    // Err(())
                                }
                            }
                        }
                        Err(e) => ws_stream.send_string(get_error_str(1, e.to_string())).await,
                    }
                }
            }
            Err(e) => ws_stream.send_string(get_error_str(2, e.to_string())).await,
        };
    }

    Ok(())
}
