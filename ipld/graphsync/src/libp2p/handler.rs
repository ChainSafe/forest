// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::codec::GraphSyncCodec;
use super::protocol::ProtocolConfig;
use crate::GraphSyncMessage;
use futures_codec::Framed;
use libp2p::swarm::{
    KeepAlive, NegotiatedSubstream, ProtocolsHandler, ProtocolsHandlerEvent,
    ProtocolsHandlerUpgrErr, SubstreamProtocol,
};
use libp2p::{InboundUpgrade, OutboundUpgrade};
use log::trace;
use smallvec::SmallVec;
use std::borrow::Cow;
use std::collections::VecDeque;
use std::io;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

// TODO move this to config option
const TIMEOUT: u64 = 5;

/// Handler implementation for GraphSync protocol.
pub struct GraphSyncHandler {
    /// Upgrade configuration for the GraphSync protocol.
    listen_protocol: SubstreamProtocol<ProtocolConfig>,

    /// Map of current substreams awaiting a response to a GraphSync request.
    inbound_substreams: VecDeque<InboundSubstreamState>,

    /// Queue of outbound substreams to open.
    dial_queue: SmallVec<[GraphSyncMessage; 4]>,

    /// Current number of concurrent outbound substreams being opened.
    dial_negotiated: u32,

    /// Maximum number of concurrent outbound substreams being opened. Value is never modified.
    _max_dial_negotiated: u32,

    /// Value to return from `connection_keep_alive`.
    keep_alive: KeepAlive,

    /// If `Some`, something bad happened and we should shut down the handler with an error.
    pending_error: Option<ProtocolsHandlerUpgrErr<io::Error>>,
}

impl GraphSyncHandler {
    /// Constructor for new RPC handler
    pub fn new(id: impl Into<Cow<'static, [u8]>>, max_transmit_size: usize) -> Self {
        Self {
            listen_protocol: SubstreamProtocol::new(ProtocolConfig::new(id, max_transmit_size)),
            ..Default::default()
        }
    }

    /// Opens an outbound substream with `upgrade`.
    #[inline]
    fn send_request(&mut self, upgrade: GraphSyncMessage) {
        self.keep_alive = KeepAlive::Yes;
        self.dial_queue.push(upgrade);
    }
}

impl Default for GraphSyncHandler {
    fn default() -> Self {
        Self {
            listen_protocol: SubstreamProtocol::new(ProtocolConfig::default()),
            inbound_substreams: Default::default(),
            dial_queue: Default::default(),
            dial_negotiated: 0,
            _max_dial_negotiated: 8,
            keep_alive: KeepAlive::Yes,
            pending_error: None,
        }
    }
}

// TODO remove allow dead_code on impl
#[allow(dead_code, clippy::large_enum_variant)]
/// State of the inbound substream, opened either by us or by the remote.
enum InboundSubstreamState {
    /// Waiting for a message from the remote. The idle state for an inbound substream.
    WaitingInput(Framed<NegotiatedSubstream, GraphSyncCodec>),
    /// The substream is being closed.
    Closing(Framed<NegotiatedSubstream, GraphSyncCodec>),
    /// An error occurred during processing.
    Poisoned,
}

impl ProtocolsHandler for GraphSyncHandler {
    type InEvent = GraphSyncMessage;
    type OutEvent = GraphSyncMessage;
    type Error = io::Error;
    type InboundProtocol = ProtocolConfig;
    type OutboundProtocol = ProtocolConfig;
    type OutboundOpenInfo = GraphSyncMessage;

    fn listen_protocol(&self) -> SubstreamProtocol<Self::InboundProtocol> {
        self.listen_protocol.clone()
    }

    fn inject_fully_negotiated_inbound(
        &mut self,
        substream: <Self::InboundProtocol as InboundUpgrade<NegotiatedSubstream>>::Output,
    ) {
        // new inbound substream. Push to back of inbound queue
        trace!("New inbound substream request");
        self.inbound_substreams
            .push_back(InboundSubstreamState::WaitingInput(substream));
    }

    fn inject_fully_negotiated_outbound(
        &mut self,
        _out: <Self::OutboundProtocol as OutboundUpgrade<NegotiatedSubstream>>::Output,
        _event: Self::OutboundOpenInfo,
    ) {
        self.dial_negotiated -= 1;

        if self.dial_negotiated == 0 && self.dial_queue.is_empty() {
            self.keep_alive = KeepAlive::Until(Instant::now() + Duration::from_secs(TIMEOUT));
        }

        // TODO handle outbound when events are emitted from service
        // self.events_out.push(out);
    }

    fn inject_event(&mut self, event: Self::InEvent) {
        self.send_request(event);
    }

    fn inject_dial_upgrade_error(
        &mut self,
        _: Self::OutboundOpenInfo,
        error: ProtocolsHandlerUpgrErr<io::Error>,
    ) {
        if self.pending_error.is_none() {
            self.pending_error = Some(error);
        }
    }

    fn connection_keep_alive(&self) -> KeepAlive {
        self.keep_alive
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
