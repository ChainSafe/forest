use super::{Message, Response};

#[derive(Debug)]
pub enum RPCRequest {
    BlocksyncRequest(Message),
}

#[derive(Debug)]
pub enum RPCResponse {
    BlocksyncResponse(Response),
}
