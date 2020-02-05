use super::blocksync_message::{Message, Response};
pub enum RPCRequest {
    BlocksyncRequest(Message)
}

pub enum RPCResponse {
    BlocksyncResponse(Response)
}
