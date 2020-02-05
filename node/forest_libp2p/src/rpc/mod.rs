pub mod protocol;
pub mod codec;
pub mod blocksync_message;
pub mod rpc_message;
pub mod handler;

use rpc_message::RPCRequest;
/// The return type used in the behaviour and the resultant event from the protocols handler.
#[derive(Debug)]
pub enum RPCEvent {
    /// An inbound/outbound request for RPC protocol. The first parameter is a sequential
    /// id which tracks an awaiting substream for the response.
    Request(RequestId, RPCRequest),
    /// A response that is being sent or has been received from the RPC protocol. The first parameter returns
    /// that which was sent with the corresponding request, the second is a single chunk of a
    /// response.
    Response(RequestId, RPCErrorResponse),
    /// An Error occurred.
    Error(RequestId, RPCError),
}

impl RPCEvent {
    pub fn id(&self) -> usize {
        match *self {
            RPCEvent::Request(id, _) => id,
            RPCEvent::Response(id, _) => id,
            RPCEvent::Error(id, _) => id,
        }
    }
}

impl std::fmt::Display for RPCEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RPCEvent::Request(id, req) => write!(f, "RPC Request(id: {}, {})", id, req),
            RPCEvent::Response(id, res) => write!(f, "RPC Response(id: {}, {})", id, res),
            RPCEvent::Error(id, err) => write!(f, "RPC Request(id: {}, error: {:?})", id, err),
        }
    }
}