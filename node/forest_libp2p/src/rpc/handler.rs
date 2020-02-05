use super::codec::{OutboundCodec, RPCError};
use super::protocol::*;
use super::rpc_message::*;
use futures::task::Poll;
use futures::{AsyncRead, AsyncWrite};
use futures_util::task::Context;
use libp2p::swarm::{ProtocolsHandler, SubstreamProtocol};
use libp2p::{InboundUpgrade, OutboundUpgrade};
use std::marker::PhantomData;
use std::pin::Pin;
use super::RPCEvent;

struct RPCHandler<TSubstream> {
    _phantom: PhantomData<TSubstream>,
}

impl<TSubstream> ProtocolsHandler for RPCHandler<TSubstream>
where
    TSubstream: AsyncWrite + AsyncRead + Unpin + Send + 'static,
{
    type InEvent = RPCEvent;
    type OutEvent = RPCEvent;
    type Error = RPCError;
    type Substream = TSubstream;
    type InboundProtocol = RPCProtocol;
    type OutboundProtocol = RPCRequest;
    type OutboundOpenInfo = RPCEvent;

    fn listen_protocol(&self) -> SubstreamProtocol<Self::InboundProtocol> {
        unimplemented!()
    }

    fn inject_fully_negotiated_inbound(&mut self, protocol: _) {
        unimplemented!()
    }

    fn inject_fully_negotiated_outbound(&mut self, protocol: _, info: Self::OutboundOpenInfo) {
        unimplemented!()
    }

    fn inject_event(&mut self, event: Self::InEvent) {
        unimplemented!()
    }

    fn inject_dial_upgrade_error(
        &mut self,
        info: Self::OutboundOpenInfo,
        error: ProtocolsHandlerUpgrErr<_>,
    ) {
        unimplemented!()
    }

    fn connection_keep_alive(&self) -> KeepAlive {
        unimplemented!()
    }

    fn poll(
        &mut self,
        cx: &mut Context<'a>,
    ) -> Poll<
        ProtocolsHandlerEvent<
            Self::OutboundProtocol,
            Self::OutboundOpenInfo,
            Self::OutEvent,
            Self::Error,
        >,
    > {
        unimplemented!()
    }
}
