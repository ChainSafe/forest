// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use log::{debug, error};
use serde::de::DeserializeOwned;

use crate::data_types::JsonRpcServerState;

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

pub const RPC_METHOD_CHAIN_HEAD_SUB: &str = "Filecoin.ChainHeadSubscription";
pub const RPC_METHOD_CHAIN_NOTIFY: &str = "Filecoin.ChainNotify";

const STREAMING_METHODS: [&str; 2] = [RPC_METHOD_CHAIN_HEAD_SUB, RPC_METHOD_CHAIN_NOTIFY];

pub fn is_streaming_method(method_name: &str) -> bool {
    STREAMING_METHODS.contains(&method_name)
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
