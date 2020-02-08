use super::{Message, Response};

#[derive(Debug, Clone)]
pub enum RPCRequest {
    BlocksyncRequest(Message),
}

impl RPCRequest {
    pub fn expect_response(&self) -> bool {
        match self {
            RPCRequest::BlocksyncRequest(_) => true,
        }
    }
}

#[derive(Debug, Clone)]
pub enum RPCResponse {
    BlocksyncResponse(Response),
}
