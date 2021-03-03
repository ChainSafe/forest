// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::data_types::JsonRpcServerState;

pub fn get_error_obj(code: i64, message: String) -> jsonrpc_v2::Error {
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

const STREAMING_METHODS: [&str; 1] = [RPC_METHOD_CHAIN_NOTIFY];

pub fn is_streaming_method(method_name: &str) -> bool {
    STREAMING_METHODS.contains(&method_name)
}

pub async fn make_rpc_call(
    rpc_server: JsonRpcServerState,
    rpc_request: jsonrpc_v2::RequestObject,
) -> Result<String, tide::Error> {
    let rpc_subscription_response = rpc_server.handle(rpc_request).await;
    Ok(serde_json::to_string_pretty(&rpc_subscription_response)?)
}
