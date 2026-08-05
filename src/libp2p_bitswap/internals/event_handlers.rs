// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::sync::Arc;

use libp2p::{PeerId, request_response};

use crate::libp2p_bitswap::{request_manager::*, *};
use crate::prelude::ShallowClone;

#[derive(Debug, Clone)]
pub enum BitswapInboundResponseEvent {
    HaveBlock(PeerId, Cid),
    DataBlock(PeerId, Cid, Vec<u8>),
}

pub fn handle_event_impl<S: BitswapStoreRead + ShallowClone + Send + Sync + 'static>(
    request_manager: &Arc<BitswapRequestManager>,
    bitswap: &mut BitswapBehaviour,
    store: &S,
    event: BitswapBehaviourEvent,
) -> anyhow::Result<()> {
    if let BitswapBehaviourEvent::Message { peer, message, .. } = event {
        match message {
            request_response::Message::Request {
                request_id: _, // `request_id` is useless here for pairing request and response
                request,
                channel,
            } => {
                // Close inbound stream immediately since `go-bitswap` does not read this
                // stream. responses will be sent over a new outbound request
                _ = bitswap.inner_mut().send_response(channel, ());
                // Serving wantlists reads the blockstore per entry; do it off
                // the swarm loop (`serve_inbound_requests`) so a large wantlist
                // can't stall all p2p handling. Inbound responses stay inline.
                let mut requests = Vec::new();
                for message in request {
                    match message {
                        BitswapMessage::Request(request) => requests.push(request),
                        BitswapMessage::Response(cid, response) => {
                            if let Some(event) = match response {
                                BitswapResponse::Have(have) => {
                                    if have {
                                        metrics::message_counter_inbound_response_have_yes().inc();
                                        Some(BitswapInboundResponseEvent::HaveBlock(peer, cid))
                                    } else {
                                        metrics::message_counter_inbound_response_have_no().inc();
                                        None
                                    }
                                }
                                BitswapResponse::Block(data) => {
                                    metrics::message_counter_inbound_response_block().inc();
                                    Some(BitswapInboundResponseEvent::DataBlock(peer, cid, data))
                                }
                            } {
                                request_manager.on_inbound_response_event(store, event);
                            }
                        }
                    }
                }
                request_manager.serve_inbound_requests(store, peer, requests);
            }
            request_response::Message::Response { .. } => {
                // Left empty by design
            }
        }
    }

    Ok(())
}

pub(in crate::libp2p_bitswap) fn handle_inbound_request<S: BitswapStoreRead>(
    store: &S,
    request: &BitswapRequest,
) -> Option<BitswapResponse> {
    if request.cancel {
        return None;
    }

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
