// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::protocol::{OutboundFramed, RPCInbound};
use super::{InboundCodec, RPCError, RPCEvent, RPCRequest, RPCResponse, RequestId};
use fnv::FnvHashMap;
use futures::prelude::*;
use futures_codec::Framed;
use libp2p::swarm::{
    KeepAlive, NegotiatedSubstream, ProtocolsHandler, ProtocolsHandlerEvent,
    ProtocolsHandlerUpgrErr, SubstreamProtocol,
};
use libp2p::{InboundUpgrade, OutboundUpgrade};
use log::error;
use smallvec::SmallVec;
use std::{
    pin::Pin,
    task::{Context, Poll},
    time::{Duration, Instant},
};

/// The time (in seconds) before a substream that is awaiting a response from the user times out.
pub const RESPONSE_TIMEOUT: u64 = 10;

pub struct RPCHandler {
    /// Upgrade configuration for RPC protocol.
    listen_protocol: SubstreamProtocol<RPCInbound>,

    /// If `Some`, something bad happened and we should shut down the handler with an error.
    pending_error: Option<ProtocolsHandlerUpgrErr<RPCError>>,

    /// Queue of events to produce in `poll()`.
    events_out: SmallVec<[RPCEvent; 4]>,

    /// Queue of outbound substreams to open.
    dial_queue: SmallVec<[RPCEvent; 4]>,

    /// Current number of concurrent outbound substreams being opened.
    dial_negotiated: u32,

    /// Map of current substreams awaiting a response to an RPC request.
    inbound_substreams: FnvHashMap<RequestId, WaitingResponse>,

    /// The vector of outbound substream states to progress.
    outbound_substreams: Vec<SubstreamState>,

    /// Sequential ID for new substreams.
    current_substream_id: RequestId,

    /// After the given duration has elapsed, an inactive connection will shutdown.
    inactive_timeout: Duration,

    /// Maximum number of concurrent outbound substreams being opened. Value is never modified.
    max_dial_negotiated: u32,

    /// Flag determining whether to maintain the connection to the peer.
    keep_alive: KeepAlive,
}

impl RPCHandler {
    /// Constructor for new RPC handler
    pub fn new(inactive_timeout: Duration) -> Self {
        RPCHandler {
            listen_protocol: SubstreamProtocol::new(RPCInbound),
            pending_error: None,
            events_out: SmallVec::new(),
            dial_queue: SmallVec::new(),
            dial_negotiated: 0,
            inbound_substreams: FnvHashMap::default(),
            outbound_substreams: Vec::new(),
            current_substream_id: 1,
            inactive_timeout,
            max_dial_negotiated: 8,
            keep_alive: KeepAlive::Yes,
        }
    }

    /// Returns the number of pending requests.
    pub fn pending_requests(&self) -> u32 {
        self.dial_negotiated + self.dial_queue.len() as u32
    }

    /// Opens an outbound substream with a request.
    pub fn send_request(&mut self, event: RPCEvent) {
        self.keep_alive = KeepAlive::Yes;

        self.dial_queue.push(event);
    }
}

impl Default for RPCHandler {
    fn default() -> Self {
        RPCHandler::new(Duration::from_secs(30))
    }
}

/// An outbound substream is waiting a response from the user.
struct WaitingResponse {
    /// The framed negotiated substream.
    substream: Framed<NegotiatedSubstream, InboundCodec>,
    /// The time when the substream is closed.
    timeout: Instant,
}

/// State of the outbound substream, opened either by us or by the remote.
enum SubstreamState {
    /// Waiting to send a message to the remote.
    PendingSend {
        substream: Framed<NegotiatedSubstream, InboundCodec>,
        response: RPCResponse,
    },
    /// Request has been sent, awaiting response
    PendingResponse {
        substream: OutboundFramed,
        event: RPCEvent,
        timeout: Instant,
    },
}

impl ProtocolsHandler for RPCHandler {
    type InEvent = RPCEvent;
    type OutEvent = RPCEvent;
    type Error = RPCError;
    type InboundProtocol = RPCInbound;
    type OutboundProtocol = RPCRequest;
    type OutboundOpenInfo = RPCEvent;

    fn listen_protocol(&self) -> SubstreamProtocol<Self::InboundProtocol> {
        self.listen_protocol.clone()
    }

    fn inject_fully_negotiated_inbound(
        &mut self,
        out: <Self::InboundProtocol as InboundUpgrade<NegotiatedSubstream>>::Output,
    ) {
        let (req, substream) = out;

        // New inbound request. Store the stream and tag the output.
        let awaiting_stream = WaitingResponse {
            substream,
            timeout: Instant::now() + Duration::from_secs(RESPONSE_TIMEOUT),
        };
        self.inbound_substreams
            .insert(self.current_substream_id, awaiting_stream);

        self.events_out
            .push(RPCEvent::Request(self.current_substream_id, req));
        self.current_substream_id += 1;
    }

    fn inject_fully_negotiated_outbound(
        &mut self,
        substream: <Self::OutboundProtocol as OutboundUpgrade<NegotiatedSubstream>>::Output,
        event: Self::OutboundOpenInfo,
    ) {
        // Decrement pending outbound substreams when processing new
        self.dial_negotiated -= 1;

        if self.dial_negotiated == 0
            && self.dial_queue.is_empty()
            && self.inbound_substreams.is_empty()
        {
            self.keep_alive = KeepAlive::Until(Instant::now() + self.inactive_timeout);
        } else {
            self.keep_alive = KeepAlive::Yes;
        }

        // add the stream to substreams if we expect a response, otherwise drop the stream
        if let RPCEvent::Request(id, req) = event {
            if req.expect_response() {
                let awaiting_stream = SubstreamState::PendingResponse {
                    substream,
                    event: RPCEvent::Request(id, req),
                    timeout: Instant::now() + Duration::from_secs(RESPONSE_TIMEOUT),
                };

                self.outbound_substreams.push(awaiting_stream);
            }
        }
    }

    fn inject_event(&mut self, event: Self::InEvent) {
        match event {
            RPCEvent::Request(_, _) => self.send_request(event),
            RPCEvent::Response(rpc_id, res) => {
                // check if the stream matching the response still exists
                if let Some(waiting_stream) = self.inbound_substreams.remove(&rpc_id) {
                    // only send one response per stream. This must be in the waiting state
                    self.outbound_substreams.push(SubstreamState::PendingSend {
                        substream: waiting_stream.substream,
                        response: res,
                    });
                }
            }
            RPCEvent::Error(_, _) => {}
        }
    }

    fn inject_dial_upgrade_error(
        &mut self,
        _: Self::OutboundOpenInfo,
        error: ProtocolsHandlerUpgrErr<
            <Self::OutboundProtocol as OutboundUpgrade<NegotiatedSubstream>>::Error,
        >,
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
        cx: &mut Context,
    ) -> Poll<
        ProtocolsHandlerEvent<
            Self::OutboundProtocol,
            Self::OutboundOpenInfo,
            Self::OutEvent,
            Self::Error,
        >,
    > {
        if let Some(err) = self.pending_error.take() {
            // Log error, shouldn't necessarily return error and drop peer here
            error!("{}", err);
        }

        // return any events that need to be reported
        if !self.events_out.is_empty() {
            return Poll::Ready(ProtocolsHandlerEvent::Custom(self.events_out.remove(0)));
        } else {
            self.events_out.shrink_to_fit();
        }

        // remove expired inbound substreams
        self.inbound_substreams
            .retain(|_, waiting_stream| Instant::now() <= waiting_stream.timeout);

        // drive streams that need to be processed
        for n in (0..self.outbound_substreams.len()).rev() {
            let stream = self.outbound_substreams.swap_remove(n);
            match stream {
                SubstreamState::PendingSend {
                    mut substream,
                    response,
                } => match Sink::poll_ready(Pin::new(&mut substream), cx) {
                    Poll::Ready(Ok(())) => {
                        // Poll until message is sent
                        if let Err(e) = Sink::start_send(Pin::new(&mut substream), response) {
                            return Poll::Ready(ProtocolsHandlerEvent::Close(e));
                        }
                        // Poll until data sent to flush the substream
                        loop {
                            match Sink::poll_flush(Pin::new(&mut substream), cx) {
                                Poll::Ready(Ok(())) => {
                                    break;
                                }
                                Poll::Ready(Err(e)) => {
                                    return Poll::Ready(ProtocolsHandlerEvent::Close(e));
                                }
                                _ => (),
                            }
                        }
                    }
                    Poll::Ready(Err(err)) => {
                        return Poll::Ready(ProtocolsHandlerEvent::Custom(RPCEvent::Error(
                            0,
                            RPCError::Custom(err.to_string()),
                        )));
                    }
                    Poll::Pending => {
                        self.outbound_substreams.push(SubstreamState::PendingSend {
                            substream,
                            response,
                        });
                    }
                },
                SubstreamState::PendingResponse {
                    mut substream,
                    event,
                    timeout,
                } => match substream.poll_next_unpin(cx) {
                    Poll::Ready(response) => {
                        match response {
                            Some(Ok(response)) => {
                                return Poll::Ready(ProtocolsHandlerEvent::Custom(
                                    RPCEvent::Response(event.id(), response),
                                ));
                            }
                            Some(Err(err)) => {
                                return Poll::Ready(ProtocolsHandlerEvent::Custom(
                                    RPCEvent::Error(event.id(), RPCError::Custom(err.to_string())),
                                ));
                            }
                            None => {
                                // stream closed early or nothing was sent
                                return Poll::Ready(ProtocolsHandlerEvent::Custom(
                                    RPCEvent::Error(
                                        event.id(),
                                        RPCError::Custom(
                                            "Stream closed early. Empty response".to_owned(),
                                        ),
                                    ),
                                ));
                            }
                        }
                    }
                    Poll::Pending => {
                        if Instant::now() < timeout {
                            self.outbound_substreams
                                .push(SubstreamState::PendingResponse {
                                    substream,
                                    event,
                                    timeout,
                                });
                        }
                    }
                },
            }
        }

        // establish outbound substreams
        if !self.dial_queue.is_empty() && self.dial_negotiated < self.max_dial_negotiated {
            self.dial_negotiated += 1;
            let event = self.dial_queue.remove(0);
            self.dial_queue.shrink_to_fit();
            if let RPCEvent::Request(id, req) = event {
                return Poll::Ready(ProtocolsHandlerEvent::OutboundSubstreamRequest {
                    protocol: SubstreamProtocol::new(req.clone()),
                    info: RPCEvent::Request(id, req),
                });
            }
        }

        Poll::Pending
    }
}
