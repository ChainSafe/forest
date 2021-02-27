use futures::stream::StreamExt;
use log::{debug, error, info};
use serde::de::DeserializeOwned;
use tide::{Error as HttpError, Request as HttpRequest};
use tide_websockets::WebSocketConnection;

use beacon::Beacon;
use blockstore::BlockStore;
use wallet::KeyStore;

use crate::chain_api::Subscription;
use crate::data_types::{JsonRpcServerState, StreamingData};
use crate::rpc_util::{get_error_str, RPC_METHOD_CHAIN_HEAD_SUB, RPC_METHOD_CHAIN_NOTIFY};
use chain::headchange_json::HeadChangeJson;

async fn make_rpc_call<T, DB, KS, B>(
    rpc_server: JsonRpcServerState,
    rpc_request: jsonrpc_v2::RequestObject,
) -> Result<T, tide::Error>
where
    T: DeserializeOwned,
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
{
    let rpc_subscription_response = rpc_server.handle(rpc_request).await;

    match rpc_subscription_response {
        jsonrpc_v2::ResponseObjects::One(rpc_subscription_params) => {
            match rpc_subscription_params {
                jsonrpc_v2::ResponseObject::Result { result, .. } => {
                    Ok(serde_json::from_value::<T>(serde_json::to_value(result)?)?)
                }
                jsonrpc_v2::ResponseObject::Error { error, .. } => match error {
                    jsonrpc_v2::Error::Provided { message, .. } => {
                        error!("Error after making RPC call: {:?}", &message);
                        Err(HttpError::from_str(
                            500,
                            format!("Error after making RPC call: {:?}", &message),
                        ))
                    }
                    _ => Err(HttpError::from_str(
                        500,
                        format!("Unknown error after making RPC call"),
                    )),
                },
            }
        }
        _ => Err(HttpError::from_str(
            500,
            format!("Unexpected response type after making RPC call"),
        )),
    }
}

pub async fn rpc_ws_handler<DB, KS, B>(
    request: HttpRequest<JsonRpcServerState>,
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

                if !request_text.is_empty() {
                    info!("RPC Request Received: {:?}", &request_text);

                    match serde_json::from_str(&request_text)
                        as Result<jsonrpc_v2::RequestObject, serde_json::Error>
                    {
                        Ok(call) => match &*call.method_ref() {
                            RPC_METHOD_CHAIN_NOTIFY => {
                                let Subscription { subscription_id } =
                                    make_rpc_call::<Subscription, DB, KS, B>(
                                        rpc_server.clone(),
                                        jsonrpc_v2::RequestObject::request()
                                            .with_method(RPC_METHOD_CHAIN_HEAD_SUB)
                                            .finish(),
                                    )
                                    .await?;

                                while let Some(event) =
                                    make_rpc_call::<Option<HeadChangeJson>, DB, KS, B>(
                                        rpc_server.clone(),
                                        jsonrpc_v2::RequestObject::request()
                                            .with_method(RPC_METHOD_CHAIN_NOTIFY)
                                            .with_id(jsonrpc_v2::Id::Num(subscription_id))
                                            .finish(),
                                    )
                                    .await?
                                {
                                    let response = StreamingData {
                                        json_rpc: "2.0",
                                        method: "xrpc.ch.val",
                                        params: (subscription_id, vec![event]),
                                    };

                                    ws_stream.send_json(&response).await?;
                                }
                            }
                            _ => {
                                error!("RPC Websocket tried handling something it shouldn't have.");

                                let response =
                                    make_rpc_call::<_, DB, KS, B>(rpc_server.clone(), call).await?;

                                ws_stream.send_json(&response).await?;
                            }
                        },
                        Err(e) => {
                            ws_stream
                                .send_string(get_error_str(1, e.to_string()))
                                .await?;
                        }
                    }
                }
            }
            Err(e) => {
                ws_stream
                    .send_string(get_error_str(2, e.to_string()))
                    .await?;
            }
        }
    }

    Ok(())
}
