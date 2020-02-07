use super::{
    InboundCodec, InboundFramed, OutboundCodec, OutboundFramed, RPCError, RPCEvent, RPCProtocol,
    RPCRequest, RPCResponse, RequestId,
};
use bytes::BytesMut;
use fnv::FnvHashMap;
use futures::prelude::*;
use futures::{AsyncRead, AsyncWrite};
use futures_codec::Framed;
use libp2p::core::Negotiated;
use libp2p::swarm::{
    KeepAlive, ProtocolsHandler, ProtocolsHandlerEvent, ProtocolsHandlerUpgrErr, SubstreamProtocol,
};
use libp2p::{InboundUpgrade, OutboundUpgrade};
use smallvec::SmallVec;
use std::{
    task::{Context, Poll},
    time::{Duration, Instant},
};

/// The time (in seconds) before a substream that is awaiting a response from the user times out.
pub const RESPONSE_TIMEOUT: u64 = 10;

pub struct RPCHandler<TSubstream>
where
    TSubstream: AsyncRead + AsyncWrite,
{
    /// Upgrade configuration for the gossipsub protocol.
    listen_protocol: SubstreamProtocol<RPCProtocol>,

    /// If `Some`, something bad happened and we should shut down the handler with an error.
    pending_error: Option<ProtocolsHandlerUpgrErr<RPCError>>,

    /// Queue of events to produce in `poll()`.
    events_out: SmallVec<[RPCEvent; 4]>,

    /// Queue of outbound substreams to open.
    dial_queue: SmallVec<[RPCEvent; 4]>,

    /// Current number of concurrent outbound substreams being opened.
    dial_negotiated: u32,

    /// Map of current substreams awaiting a response to an RPC request.
    inbound_substreams: FnvHashMap<RequestId, WaitingResponse<TSubstream>>,

    /// The single long-lived outbound substream.
    outbound_substreams: Vec<OutboundSubstreamState<TSubstream>>,

    /// Queue of values that we want to send to the remote.
    send_queue: SmallVec<[RPCRequest; 16]>,

    /// Sequential ID for new substreams.
    current_substream_id: RequestId,

    /// After the given duration has elapsed, an inactive connection will shutdown.
    inactive_timeout: Duration,

    /// Flag determining whether to maintain the connection to the peer.
    keep_alive: KeepAlive,
}

impl<TSubstream> RPCHandler<TSubstream>
where
    TSubstream: AsyncRead + AsyncWrite,
{
    /// Opens an outbound substream with a request.
    pub fn send_request(&mut self, rpc_event: RPCEvent) {
        self.keep_alive = KeepAlive::Yes;

        self.dial_queue.push(rpc_event);
    }
}

/// An outbound substream is waiting a response from the user.
struct WaitingResponse<TSubstream> {
    /// The framed negotiated substream.
    // TODO would be nice to not specify type explicitly here
    substream: Framed<Negotiated<TSubstream>, InboundCodec>,
    /// The time when the substream is closed.
    timeout: Instant,
}

/// State of the outbound substream, opened either by us or by the remote.
enum OutboundSubstreamState<TSubstream>
where
    TSubstream: AsyncRead + AsyncWrite,
{
    /// Waiting to send a message to the remote.
    PendingSend {
        // TODO verify codec
        substream: Framed<Negotiated<TSubstream>, InboundCodec>,
        response: RPCResponse,
    },
    /// Request has been sent, awaiting response
    PendingResponse {
        substream: OutboundFramed<TSubstream>,
        event: RPCEvent,
        timeout: Instant,
    },
}

impl<TSubstream> ProtocolsHandler for RPCHandler<TSubstream>
where
    TSubstream: AsyncWrite + AsyncRead + Unpin + Send + 'static,
{
    type InEvent = RPCEvent;
    type OutEvent = RPCEvent;
    type Error = ProtocolsHandlerUpgrErr<RPCError>;
    type Substream = TSubstream;
    type InboundProtocol = RPCProtocol;
    type OutboundProtocol = RPCProtocol;
    type OutboundOpenInfo = RPCEvent;

    fn listen_protocol(&self) -> SubstreamProtocol<Self::InboundProtocol> {
        self.listen_protocol.clone()
    }

    fn inject_fully_negotiated_inbound(
        &mut self,
        out: <Self::InboundProtocol as InboundUpgrade<Negotiated<Self::Substream>>>::Output,
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
        _substream: <Self::OutboundProtocol as OutboundUpgrade<Negotiated<TSubstream>>>::Output,
        _rpc_event: Self::OutboundOpenInfo,
    ) {
        // TODO verify outbound gets handled
        self.dial_negotiated -= 1;

        if self.dial_negotiated == 0
            && self.dial_queue.is_empty()
            && self.inbound_substreams.is_empty()
        {
            self.keep_alive = KeepAlive::Until(Instant::now() + self.inactive_timeout);
        } else {
            self.keep_alive = KeepAlive::Yes;
        }

        // TODO keep stream open if other protocols require it in future
    }

    fn inject_event(&mut self, rpc_event: Self::InEvent) {
        match rpc_event {
            RPCEvent::Request(_, _) => self.send_request(rpc_event),
            RPCEvent::Response(rpc_id, res) => {
                // check if the stream matching the response still exists
                if let Some(waiting_stream) = self.inbound_substreams.remove(&rpc_id) {
                    // only send one response per stream. This must be in the waiting state.
                    self.outbound_substreams
                        .push(OutboundSubstreamState::PendingSend {
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
        _: ProtocolsHandlerUpgrErr<
            <Self::OutboundProtocol as OutboundUpgrade<Self::Substream>>::Error,
        >,
    ) {
        // Can maybe ignore
        println!("Ignoring dial error");
    }

    fn connection_keep_alive(&self) -> KeepAlive {
        self.keep_alive
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
        todo!();
    }
}
