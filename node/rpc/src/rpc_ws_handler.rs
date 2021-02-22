use crate::data_types::{State, StreamingData};
use crate::rpc_util::get_error;
use async_std::sync::Arc;
use beacon::Beacon;
use blockstore::BlockStore;
use chain::headchange_json::HeadChangeJson;
use futures::stream::StreamExt;
use jsonrpc_v2::Server as JsonRpcServer;
use log::{debug, info};
use rpc_types::{Id, JsonRpcRequestObject};
use serde::Serialize;
use tide::Request;
use tide_websockets::WebSocketConnection;
use wallet::KeyStore;

const CHAIN_NOTIFY_METHOD_NAME: &str = "Filecoin.ChainNotify";

pub async fn rpc_ws_handler<DB, KS, B>(
    request: Request<JsonRpcServer<State<DB, KS, B>>>,
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
                                CHAIN_NOTIFY_METHOD_NAME => {
                                    let chain_notify_count_curr =
                                        chain_notify_count.fetch_add(1usize);

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
                                    let response = SubscribeChannelIDResponse {
                                        json_rpc: "2.0",
                                        result: chain_notify_count.load(),
                                        id: call.id.flatten().unwrap_or(Id::Null),
                                    };

                                    ws_stream.send_json(&response).await?; // TODO: handle send error

                                    while let Some(event) = head_changes.next().await {
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
                                _ => {
                                    let rpc_response = rpc_server.handle(call).await;

                                    match rpc_response {}

                                    Ok(())
                                }
                            }
                        }
                        Err(e) => ws_stream.send_string(get_error(1, e.to_string())).await,
                    }
                }
            }
            Err(e) => ws_stream.send_string(get_error(2, e.to_string())).await,
        };
    }

    Ok(())
}
