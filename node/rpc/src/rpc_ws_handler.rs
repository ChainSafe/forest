use futures::stream::StreamExt;
use log::{debug, error, info};
use tide_websockets::WebSocketConnection;

use beacon::Beacon;
use blockstore::BlockStore;
use wallet::KeyStore;

use crate::chain_api::Subscription;
use crate::data_types::{JsonRpcServerState, StreamingData};
use crate::rpc_util::{
    get_error_str, make_rpc_call, RPC_METHOD_CHAIN_HEAD_SUB, RPC_METHOD_CHAIN_NOTIFY,
};
use chain::headchange_json::HeadChangeJson;

pub async fn rpc_ws_handler<DB, KS, B>(
    request: tide::Request<JsonRpcServerState>,
    mut ws_stream: WebSocketConnection,
) -> Result<(), tide::Error>
where
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
{
    let rpc_server = request.state();

    debug!("Accepted WS connection from {:?}", request.remote());

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
                                let Subscription { subscription_id } = serde_json::from_str(
                                    &make_rpc_call(
                                        rpc_server.clone(),
                                        jsonrpc_v2::RequestObject::request()
                                            .with_method(RPC_METHOD_CHAIN_HEAD_SUB)
                                            .finish(),
                                    )
                                    .await?,
                                )?;

                                while let Some(event) =
                                    serde_json::from_str::<Option<HeadChangeJson>>(
                                        &make_rpc_call(
                                            rpc_server.clone(),
                                            jsonrpc_v2::RequestObject::request()
                                                .with_method(RPC_METHOD_CHAIN_NOTIFY)
                                                .with_id(jsonrpc_v2::Id::Num(subscription_id))
                                                .finish(),
                                        )
                                        .await?,
                                    )?
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
                                error!("RPC WS called method: {}", call.method_ref());

                                let response = make_rpc_call(rpc_server.clone(), call).await?;

                                ws_stream.send_string(response).await?;
                            }
                        },
                        Err(e) => {
                            error!("Error deserializing WS request payload.");
                            ws_stream
                                .send_string(get_error_str(1, e.to_string()))
                                .await?;
                        }
                    }
                }
            }
            Err(e) => {
                error!("Error in WS socket stream. (Client possibly disconnected)");
                ws_stream
                    .send_string(get_error_str(2, e.to_string()))
                    .await?;
            }
        }
    }

    Ok(())
}
