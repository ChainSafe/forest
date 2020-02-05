use super::codec::{InboundCodec, OutboundCodec, RPCError};
use futures::prelude::*;
use futures::{AsyncRead, AsyncWrite};
use futures_codec::Framed;
use libp2p::core::UpgradeInfo;
use libp2p::{InboundUpgrade, OutboundUpgrade};
use std::pin::Pin;

// const MAX_RPC_SIZE: u64 = 4_194_304;

#[derive(Debug, Clone)]
pub struct RPCProtocol;

impl UpgradeInfo for RPCProtocol {
    type Info = &'static [u8];
    type InfoIter = Vec<Self::Info>;

    fn protocol_info(&self) -> Self::InfoIter {
        vec![b"/fil/sync/blk/0.0.1"]
    }
}

impl<TSocket> InboundUpgrade<TSocket> for RPCProtocol
where
    TSocket: AsyncWrite + AsyncRead + Unpin + Send + 'static,
{
    type Output = Framed<TSocket, InboundCodec>;
    type Error = RPCError;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Output, Self::Error>> + Send>>;

    fn upgrade_inbound(self, socket: TSocket, _: Self::Info) -> Self::Future {
        Box::pin(future::ok(Framed::new(socket, InboundCodec)))
    }
}

impl<TSocket> OutboundUpgrade<TSocket> for RPCProtocol
where
    TSocket: AsyncWrite + AsyncRead + Unpin + Send + 'static,
{
    type Output = Framed<TSocket, OutboundCodec>;
    type Error = RPCError;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Output, Self::Error>> + Send>>;

    fn upgrade_outbound(self, socket: TSocket, _: Self::Info) -> Self::Future {
        Box::pin(future::ok(Framed::new(socket, OutboundCodec)))
    }
}
