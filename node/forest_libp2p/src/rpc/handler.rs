use super::codec::{OutboundCodec, RPCError};
use super::protocol::*;
use super::rpc_message::*;
use futures::task::Poll;
use futures::{AsyncRead, AsyncWrite};
use futures_util::task::Context;
use libp2p::swarm::{ProtocolsHandler, SubstreamProtocol, ProtocolsHandlerUpgrErr, KeepAlive, ProtocolsHandlerEvent};
use libp2p::{InboundUpgrade, OutboundUpgrade};
use libp2p::core::Negotiated;
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
    type OutboundProtocol = RPCProtocol;
    type OutboundOpenInfo = RPCEvent;

    fn listen_protocol(&self) -> SubstreamProtocol<Self::InboundProtocol> {
        unimplemented!()
    }

    fn inject_fully_negotiated_inbound(&mut self, protocol: <Self::InboundProtocol as InboundUpgrade<Negotiated<Self::Substream>>>::Output) {
        unimplemented!()
    }

    fn inject_fully_negotiated_outbound(&mut self, protocol: <Self::OutboundProtocol as OutboundUpgrade<Negotiated<TSubstream>>>::Output, info: Self::OutboundOpenInfo) {
        unimplemented!()
    }

    fn inject_event(&mut self, event: Self::InEvent) {
        unimplemented!()
    }

    fn inject_dial_upgrade_error(
        &mut self,
        info: Self::OutboundOpenInfo,
        error: ProtocolsHandlerUpgrErr<<Self::OutboundProtocol as OutboundUpgrade<Self::Substream>>::Error>,
    ) {
        unimplemented!()
    }

    fn connection_keep_alive(&self) -> KeepAlive {
        unimplemented!()
    }

    fn poll(
        &mut self,
        cx: &mut Context,
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
