// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::*;
use libipld::Block;
use libp2p::{
    request_response::{RequestResponseEvent, RequestResponseMessage},
    PeerId,
};

pub enum BitswapInboundResponseEvent {
    HaveBlock(PeerId, Cid),
    BlockSaved(PeerId, Cid),
}

// Note: This method performs db IO syncronously to reduce complexity
pub async fn handle_event<S: BitswapStore>(
    bitswap: &mut BitswapBehaviour,
    store: &S,
    event: BitswapBehaviourEvent,
    inbound_response_tx: flume::Sender<BitswapInboundResponseEvent>,
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
                                if let Some(response) =
                                    handle_inbound_request(store, &request).await
                                {
                                    bitswap.send_response(&peer, (request.cid, response)).await;
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
                                        // Avoid duplicate writes
                                        // but still emit event
                                        if let Ok(true) = store.contains(&cid) {
                                            Some(BitswapInboundResponseEvent::BlockSaved(peer, cid))
                                        } else {
                                            match Block::new(cid, data) {
                                                Ok(block) => match store.insert(&block) {
                                                    Ok(()) => {
                                                        metrics::message_counter_inbound_response_block_update_db().inc();
                                                        Some(
                                                            BitswapInboundResponseEvent::BlockSaved(
                                                                peer, cid,
                                                            ),
                                                        )
                                                    }
                                                    Err(e) => {
                                                        metrics::message_counter_inbound_response_block_update_db_failure().inc();
                                                        warn!("Failed to update db: {e}, cid: {cid}, data: {:?}",block.data());
                                                        None
                                                    }
                                                },
                                                Err(e) => {
                                                    // TODO: log data
                                                    warn!("Failed to construct block: {e}, cid: {cid}");
                                                    None
                                                }
                                            }
                                        }
                                    }
                                } {
                                    if inbound_response_tx.send_async(event).await.is_err() {
                                        warn!("Failed to send inbound response event");
                                    }
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

async fn handle_inbound_request<S: BitswapStore>(
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
