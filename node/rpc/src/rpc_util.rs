// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use log::{debug, error};
use serde::de::DeserializeOwned;
use tide::http::headers::HeaderValues;

use beacon::Beacon;
use blockstore::BlockStore;
use rpc_api::{
    auth_api::*, chain_api::*, check_access, data_types::JsonRpcServerState, ACCESS_MAP,
};

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
        Err(err) => format!("Failed to serialize error data. Error was: {}", err),
    }
}

const STREAMING_METHODS: [&str; 2] = [CHAIN_HEAD_SUBSCRIPTION, CHAIN_NOTIFY];

pub fn is_streaming_method(method_name: &str) -> bool {
    STREAMING_METHODS.contains(&method_name)
}

pub async fn check_permissions<DB, B>(
    rpc_server: JsonRpcServerState,
    method: &str,
    authorization_header: Option<HeaderValues>,
) -> Result<(), tide::Error>
where
    DB: BlockStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
{
    let claims = match authorization_header
        .and_then(|header_values| header_values.get(0).cloned())
        .map(|token| token.to_string())
    {
        Some(token) => {
            debug!("JWT from HTTP Header: {}", token);

            let (_, claims) = call_rpc::<Vec<String>>(
                rpc_server,
                jsonrpc_v2::RequestObject::request()
                    .with_method(AUTH_VERIFY)
                    .with_params(vec![token])
                    .finish(),
            )
            .await?;

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
                Err(tide::Error::from_str(403, "Forbidden"))
            }
        }
        None => Err(tide::Error::from_str(404, "Not Found")),
    }
}

pub fn get_auth_header(
    request: tide::Request<JsonRpcServerState>,
) -> (Option<HeaderValues>, tide::Request<JsonRpcServerState>) {
    (request.header("Authorization").cloned(), request)
}

// Calls an RPC method and returns the full response as a string.
pub async fn call_rpc_str(
    rpc_server: JsonRpcServerState,
    rpc_request: jsonrpc_v2::RequestObject,
) -> Result<String, tide::Error> {
    let rpc_subscription_response = rpc_server.handle(rpc_request).await;
    Ok(serde_json::to_string(&rpc_subscription_response)?)
}

// Returns both the RPC response string and the result value in a tuple.
pub async fn call_rpc<T>(
    rpc_server: JsonRpcServerState,
    rpc_request: jsonrpc_v2::RequestObject,
) -> Result<(String, T), tide::Error>
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
                        Err(tide::Error::from_str(500, msg))
                    }
                    jsonrpc_v2::Error::Full { code, message, .. } => {
                        let msg = format!(
                            "Unknown error after making RPC call. Code: {}. Error: {:?} ",
                            code, message
                        );
                        error!("RPC call error: {}", msg);
                        Err(tide::Error::from_str(500, msg))
                    }
                },
            }
        }
        _ => {
            let msg = "Unexpected response type after making RPC call";
            error!("RPC call error: {}", msg);
            Err(tide::Error::from_str(500, msg))
        }
    }
}
