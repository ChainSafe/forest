use super::blocksync_message::{Message, Response};
#[derive(Debug)]
pub enum RPCRequest {
    BlocksyncRequest(Message)
}

#[derive(Debug)]
pub enum RPCResponse {
    BlocksyncResponse(Response)
}
