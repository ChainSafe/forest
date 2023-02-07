// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use forest_beacon::Beacon;
use forest_rpc_api::{auth_api::*, check_access, data_types::JsonRpcServerState, ACCESS_MAP};
use fvm_ipld_blockstore::Blockstore;
use http::{HeaderMap, HeaderValue, StatusCode};
use log::{debug, error};
use serde::de::DeserializeOwned;

pub fn get_error_obj(code: i64, message: String) -> jsonrpc_v2::Error {
    debug!(
        "Error object created with code {} and message {}",
        code, message
    );
    jsonrpc_v2::Error::Full {
        code,
        message,
        data: None,
    }
}

pub fn get_error_res(code: i64, message: String) -> jsonrpc_v2::ResponseObject {
    jsonrpc_v2::ResponseObject::Error {
        jsonrpc: jsonrpc_v2::V2,
        error: get_error_obj(code, message),
        id: jsonrpc_v2::Id::Null,
    }
}

pub fn get_error_str(code: i64, message: String) -> String {
    match serde_json::to_string(&get_error_res(code, message)) {
        Ok(err_str) => err_str,
        Err(err) => format!("Failed to serialize error data. Error was: {err}"),
    }
}

const STREAMING_METHODS: [&str; 0] = [];

pub fn is_streaming_method(method_name: &str) -> bool {
    STREAMING_METHODS.contains(&method_name)
}

pub async fn check_permissions<DB, B>(
    rpc_server: JsonRpcServerState,
    method: &str,
    authorization_header: Option<HeaderValue>,
) -> Result<(), (StatusCode, String)>
where
    DB: Blockstore,
    B: Beacon,
{
    let claims = match authorization_header {
        Some(token) => {
            let token = token
                .to_str()
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            debug!("JWT from HTTP Header: {}", token);
            let (_, claims) = call_rpc::<Vec<String>>(
                rpc_server,
                jsonrpc_v2::RequestObject::request()
                    .with_method(AUTH_VERIFY)
                    .with_params(vec![token])
                    .finish(),
            )
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

            debug!("Decoded JWT Claims: {:?}", claims);

            claims
        }
        // If no token is passed, assume read behavior
        None => vec!["read".to_owned()],
    };

    match ACCESS_MAP.get(&method) {
        Some(access) => {
            if check_access(access, &claims) {
                Ok(())
            } else {
                Err((StatusCode::FORBIDDEN, "Forbidden".into()))
            }
        }
        None => Err((StatusCode::NOT_FOUND, "Not Found".into())),
    }
}

pub fn get_auth_header(headers: HeaderMap) -> Option<HeaderValue> {
    headers.get("Authorization").cloned()
}

// Calls an RPC method and returns the full response as a string.
pub async fn call_rpc_str(
    rpc_server: JsonRpcServerState,
    rpc_request: jsonrpc_v2::RequestObject,
) -> anyhow::Result<String> {
    let rpc_subscription_response = rpc_server.handle(rpc_request).await;
    Ok(serde_json::to_string(&rpc_subscription_response)?)
}

// Returns both the RPC response string and the result value in a tuple.
pub async fn call_rpc<T>(
    rpc_server: JsonRpcServerState,
    rpc_request: jsonrpc_v2::RequestObject,
) -> anyhow::Result<(String, T)>
where
    T: DeserializeOwned,
{
    debug!("RPC invoked");

    let rpc_subscription_response = rpc_server.handle(rpc_request).await;

    debug!("RPC request received");

    match &rpc_subscription_response {
        jsonrpc_v2::ResponseObjects::One(rpc_subscription_params) => {
            match rpc_subscription_params {
                jsonrpc_v2::ResponseObject::Result { result, .. } => {
                    let response_str = serde_json::to_string(&rpc_subscription_response)?;
                    debug!("RPC Response: {:?}", response_str);
                    Ok((
                        response_str,
                        serde_json::from_value::<T>(serde_json::to_value(result)?)?,
                    ))
                }
                jsonrpc_v2::ResponseObject::Error { error, .. } => match error {
                    jsonrpc_v2::Error::Provided { message, code } => {
                        let msg = format!(
                            "Error after making RPC call. Code: {}. Error: {:?}",
                            code, &message
                        );
                        error!("RPC call error: {}", msg);
                        anyhow::bail!(msg)
                    }
                    jsonrpc_v2::Error::Full { code, message, .. } => {
                        let msg = format!(
                            "Unknown error after making RPC call. Code: {code}. Error: {message:?} "
                        );
                        error!("RPC call error: {}", msg);
                        anyhow::bail!(msg)
                    }
                },
            }
        }
        _ => {
            let msg = "Unexpected response type after making RPC call";
            error!("RPC call error: {}", msg);
            anyhow::bail!(msg)
        }
    }
}
