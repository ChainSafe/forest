// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc_api::data_types::JsonRpcServerState;
use axum::response::IntoResponse;
use http::{HeaderMap, StatusCode};
use jsonrpc_v2::RequestObject as JsonRpcRequestObject;

use crate::rpc::rpc_util::{call_rpc_str, check_permissions, get_auth_header, is_streaming_method};

pub async fn rpc_http_handler(
    headers: HeaderMap,
    axum::extract::State(rpc_server): axum::extract::State<JsonRpcServerState>,
    axum::Json(rpc_call): axum::Json<JsonRpcRequestObject>,
) -> impl IntoResponse {
    let response_headers = [("content-type", "application/json-rpc;charset=utf-8")];
    if let Err((code, msg)) = check_permissions(
        rpc_server.clone(),
        rpc_call.method_ref(),
        get_auth_header(headers),
    )
    .await
    {
        return (code, response_headers, msg);
    }

    if is_streaming_method(rpc_call.method_ref()) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            response_headers,
            "This endpoint cannot handle streaming methods".into(),
        );
    }

    match call_rpc_str(rpc_server.clone(), rpc_call).await {
        Ok(result) => (StatusCode::OK, response_headers, result),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            response_headers,
            err.to_string(),
        ),
    }
}
