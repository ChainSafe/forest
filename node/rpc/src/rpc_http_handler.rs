use tide::http::{format_err, Error as HttpError, Method};
use tide::{Request as HttpRequest, Response as HttpResponse, Result};

use beacon::Beacon;
use blockstore::BlockStore;
use wallet::KeyStore;

use crate::data_types::JsonRpcServerState;
use crate::rpc_util::is_streaming_method;
use rpc_types::JsonRpcRequestObject;

pub async fn rpc_http_handler<DB, KS, B>(
    mut http_request: HttpRequest<JsonRpcServerState>,
) -> Result
where
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
{
    if http_request.method() != Method::Post {
        return Err(format_err!("HTTP JSON RPC calls must use POST HTTP method"));
    } else if let Some(content_type) = http_request.content_type() {
        match content_type.essence() {
            "application/json-rpc" => {}
            "application/json" => {}
            "application/jsonrequest" => {}
            _ => {
                return Err(format_err!(
                    "HTTP JSON RPC calls must provide an appropriate Content-Type header"
                ));
            }
        }
    }

    let rpc_call: JsonRpcRequestObject = http_request.body_json().await?;

    if is_streaming_method(rpc_call.method.to_string()) {
        return Err(HttpError::from_str(
            500,
            "This endpoint should not handle streaming methods",
        ));
    }

    let http_request_bytes = http_request.body_bytes().await.unwrap();
    let rpc_server = http_request.state();
    let rpc_response = rpc_server.handle(http_request_bytes.as_ref()).await;
    let http_response = HttpResponse::builder(200)
        .body(serde_json::to_string(&rpc_response).unwrap())
        .content_type("application/json-rpc;charset=utf-8")
        .build();

    Ok(http_response)
}
