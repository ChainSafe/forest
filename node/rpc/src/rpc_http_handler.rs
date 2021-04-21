// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use jsonrpc_v2::RequestObject as JsonRpcRequestObject;
use tide::http::{format_err, Error as HttpError, Method};

use beacon::Beacon;
use blockstore::BlockStore;
use wallet::KeyStore;

use crate::data_types::JsonRpcServerState;
use crate::rpc_util::{call_rpc_str, check_permissions, get_auth_header, is_streaming_method};

pub async fn rpc_http_handler<DB, KS, B>(request: tide::Request<JsonRpcServerState>) -> tide::Result
where
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
{
    let (auth_header, mut request) = get_auth_header(request);
    let rpc_call: JsonRpcRequestObject = request.body_json().await?;
    let rpc_server = request.state();

    if request.method() != Method::Post {
        return Err(format_err!("HTTP JSON RPC calls must use POST HTTP method"));
    } else if let Some(content_type) = request.content_type() {
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

    check_permissions::<DB, KS, B>(rpc_server.clone(), rpc_call.method_ref(), auth_header).await?;

    if is_streaming_method(rpc_call.method_ref()) {
        return Err(HttpError::from_str(
            500,
            "This endpoint cannot handle streaming methods",
        ));
    }

    let result = call_rpc_str(rpc_server.clone(), rpc_call).await?;
    let response = tide::Response::builder(200)
        .body(result)
        .content_type("application/json-rpc;charset=utf-8")
        .build();

    Ok(response)
}
