// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::protocol::ProtocolConfig;
use crate::GraphSyncMessage;
use libp2p::swarm::{
    KeepAlive, NegotiatedSubstream, ProtocolsHandler, ProtocolsHandlerEvent,
    ProtocolsHandlerUpgrErr, SubstreamProtocol,
};
use libp2p::{InboundUpgrade, OutboundUpgrade};
use std::io;
use std::task::{Context, Poll};

pub struct GraphSyncHandler {
    // TODO
}

impl GraphSyncHandler {
    /// Constructor for new RPC handler
    pub fn new() -> Self {
        // TODO
        GraphSyncHandler {}
    }
}

impl Default for GraphSyncHandler {
    fn default() -> Self {
        GraphSyncHandler::new()
    }
}

impl ProtocolsHandler for GraphSyncHandler {
    type InEvent = GraphSyncMessage;
    type OutEvent = GraphSyncMessage;
    type Error = io::Error;
    type InboundProtocol = ProtocolConfig;
    type OutboundProtocol = ProtocolConfig;
    type OutboundOpenInfo = GraphSyncMessage;

    fn listen_protocol(&self) -> SubstreamProtocol<Self::InboundProtocol> {
        todo!()
    }

    fn inject_fully_negotiated_inbound(
        &mut self,
        _out: <Self::InboundProtocol as InboundUpgrade<NegotiatedSubstream>>::Output,
    ) {
        todo!()
    }

    fn inject_fully_negotiated_outbound(
        &mut self,
        _substream: <Self::OutboundProtocol as OutboundUpgrade<NegotiatedSubstream>>::Output,
        _event: Self::OutboundOpenInfo,
    ) {
        todo!()
    }

    fn inject_event(&mut self, _event: Self::InEvent) {
        todo!()
    }

    fn inject_dial_upgrade_error(
        &mut self,
        _: Self::OutboundOpenInfo,
        _error: ProtocolsHandlerUpgrErr<
            <Self::OutboundProtocol as OutboundUpgrade<NegotiatedSubstream>>::Error,
        >,
    ) {
        todo!()
    }

    fn connection_keep_alive(&self) -> KeepAlive {
        todo!()
    }

    #[allow(clippy::type_complexity)]
    fn poll(
        &mut self,
        _cx: &mut Context,
    ) -> Poll<
        ProtocolsHandlerEvent<
            Self::OutboundProtocol,
            Self::OutboundOpenInfo,
            Self::OutEvent,
            Self::Error,
        >,
    > {
        todo!()
    }
}
