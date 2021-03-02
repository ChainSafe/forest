use jsonrpc_v2::{Error as JSONRPCError, Id, ResponseObject, V2};
use log::error;
use serde::de::DeserializeOwned;
use tide::Error as HttpError;

use beacon::Beacon;
use blockstore::BlockStore;
use wallet::KeyStore;

use crate::data_types::JsonRpcServerState;

pub fn get_error_obj(code: i64, message: String) -> JSONRPCError {
    JSONRPCError::Full {
        code,
        message,
        data: None,
    }
}

pub fn get_error_res(code: i64, message: String) -> ResponseObject {
    ResponseObject::Error {
        jsonrpc: V2,
        error: get_error_obj(code, message),
        id: Id::Null,
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
    STREAMING_METHODS.contains(&method_name.as_ref())
}

pub async fn make_rpc_call<T, DB, KS, B>(
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
                    jsonrpc_v2::Error::Provided { message, code } => {
                        let msg = format!(
                            "Error after making RPC call. Code: {}. Error: {:?}",
                            code, &message
                        );
                        error!("{}", &msg);
                        Err(HttpError::from_str(500, msg))
                    }
                    jsonrpc_v2::Error::Full { code, message, .. } => {
                        let msg = format!(
                            "Unknown error after making RPC call. Code: {}. Error: {:?} ",
                            code, message
                        );
                        error!("{}", &msg);
                        Err(HttpError::from_str(500, msg))
                    }
                },
            }
        }
        _ => Err(HttpError::from_str(
            500,
            format!("Unexpected response type after making RPC call"),
        )),
    }
}
