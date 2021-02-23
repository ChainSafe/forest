use jsonrpc_v2::{Error as JSONRPCError, Id, ResponseObject, V2};

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

pub fn is_streaming_method(method_name: String) -> bool {
    STREAMING_METHODS.contains(&method_name.as_ref())
}
