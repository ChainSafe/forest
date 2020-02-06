mod blocksync_message;
mod codec;
mod handler;
mod protocol;
mod rpc_message;

pub use blocksync_message::*;
pub use codec::*;
pub use handler::*;
pub use protocol::*;
pub use rpc_message::*;

use crate::rpc::rpc_message::RPCResponse;
use rpc_message::RPCRequest;

/// The return type used in the behaviour and the resultant event from the protocols handler.
#[derive(Debug)]
pub enum RPCEvent {
    /// An inbound/outbound request for RPC protocol. The first parameter is a sequential
    /// id which tracks an awaiting substream for the response.
    Request(RPCRequest),
    /// A response that is being sent or has been received from the RPC protocol. The first parameter returns
    /// that which was sent with the corresponding request, the second is a single chunk of a
    /// response.
    Response(RPCResponse),
}

// impl RPCEvent {
//     pub fn id(&self) -> usize {
//         match *self {
//             RPCEvent::Request(request) => request,
//             RPCEvent::Response(id) => id,
//         }
//     }
// }

impl std::fmt::Display for RPCEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RPCEvent::Request(req) => write!(f, "RPC Request(req: {:?})", req),
            RPCEvent::Response(res) => write!(f, "RPC Response(res: {:?})", res),
        }
    }
}
