use jsonrpc_v2::{Error as JSONRPCError, Id, ResponseObject, V2};

pub fn get_error(code: i64, message: String) -> ResponseObject {
    ResponseObject::Error {
        jsonrpc: V2,
        error: JSONRPCError::Full {
            code,
            message,
            data: None,
        },
        id: Id::Null,
    }
}

pub fn get_error_str(code: i64, message: String) -> String {
    match serde_json::to_string(&get_error(code, message)) {
        Ok(err_str) => err_str,
        Err(err) => format!("Failed to serialize error data. Error was: {}", err),
    }
}
