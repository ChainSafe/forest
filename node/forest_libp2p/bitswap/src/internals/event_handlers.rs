// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::*;
use libp2p::{
    request_response::{RequestResponseEvent, RequestResponseMessage},
    PeerId,
};
use std::sync::Arc;

pub enum BitswapInboundResponseEvent {
    HaveBlock(PeerId, Cid),
    DataBlock(PeerId, Cid, Vec<u8>),
}

// Note: This method performs db IO syncronously to reduce complexity
pub fn handle_event_impl<S: BitswapStore>(
    request_manager: &Arc<BitswapRequestManager>,
    bitswap: &mut BitswapBehaviour,
    store: &S,
    event: BitswapBehaviourEvent,
) -> anyhow::Result<()> {
    match event {
        BitswapBehaviourEvent::Inner(RequestResponseEvent::Message { peer, message }) => {
            match message {
                RequestResponseMessage::Request {
                    request_id: _, // `request_id` is useless here for pairing request and response
                    request,
                    channel,
                } => {
                    // Close inbound stream immediately since `go-bitswap` does not read this stream.
                    // responses will be sent over a new outbound request
                    _ = bitswap.inner.send_response(channel, ());
                    for message in request {
                        match message {
                            BitswapMessage::Request(request) => {
                                if let Some(response) = handle_inbound_request(store, &request) {
                                    bitswap.send_response(&peer, (request.cid, response));
                                }
                            }
                            BitswapMessage::Response(cid, response) => {
                                if let Some(event) = match response {
                                    BitswapResponse::Have(have) => {
                                        if have {
                                            metrics::message_counter_inbound_response_have_yes()
                                                .inc();
                                            Some(BitswapInboundResponseEvent::HaveBlock(peer, cid))
                                        } else {
                                            metrics::message_counter_inbound_response_have_no()
                                                .inc();
                                            None
                                        }
                                    }
                                    BitswapResponse::Block(data) => {
                                        metrics::message_counter_inbound_response_block().inc();
                                        Some(BitswapInboundResponseEvent::DataBlock(
                                            peer, cid, data,
                                        ))
                                    }
                                } {
                                    request_manager.on_inbound_response_event(store, event);
                                }
                            }
                        }
                    }
                }
                RequestResponseMessage::Response { .. } => {
                    // Left empty by design
                }
            }
        }
        BitswapBehaviourEvent::Inner(_) => {
            // TODO: trace
        }
    };

    Ok(())
}

fn handle_inbound_request<S: BitswapStore>(
    store: &S,
    request: &BitswapRequest,
) -> Option<BitswapResponse> {
    match request.ty {
        RequestType::Have => {
            metrics::message_counter_inbound_request_have().inc();
            let have = store.contains(&request.cid).ok().unwrap_or_default();
            if have || request.send_dont_have {
                Some(BitswapResponse::Have(have))
            } else {
                None
            }
        }
        RequestType::Block => {
            metrics::message_counter_inbound_request_block().inc();
            let block = store.get(&request.cid).ok().unwrap_or_default();
            if let Some(data) = block {
                Some(BitswapResponse::Block(data))
            } else if request.send_dont_have {
                Some(BitswapResponse::Have(false))
            } else {
                None
            }
        }
    }
}
