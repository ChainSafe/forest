// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{LinkTracker, ResponseBuilder};
use crate::{ExtensionData, GraphSyncResponse, RequestID, ResponseStatusCode, MAX_BLOCK_SIZE};
use async_trait::async_trait;
use cid::Cid;
use libp2p::core::PeerId;

/// Handles batching, deduping, and sending responses for a given peer across multiple requests.
pub struct PeerResponseSender {
    peer: PeerId,
    link_tracker: LinkTracker,
    response_builders: Vec<ResponseBuilder>,
}

impl PeerResponseSender {
    /// Creates a new peer response sender.
    pub fn new(peer: PeerId) -> Self {
        Self {
            peer,
            link_tracker: LinkTracker::new(),
            response_builders: Vec::new(),
        }
    }

    /// Sends a given link for a given request ID across the wire, as well as its corresponding
    /// block if the block is present and has not already been sent.
    /// Returns true if the block has not already been sent and is thus added to a response.
    pub fn send_response(&mut self, id: RequestID, link: Cid, data: Option<Vec<u8>>) -> bool {
        let block_is_present = data.is_some();
        let block_size = data.as_ref().map_or(0, |vec| vec.len());

        // if we've traversed this block before for this peer (not necessarily for this particular request),
        // there's no need to send it again
        let block = data.filter(|_| self.link_tracker.block_ref_count(&link) == 0);
        self.link_tracker
            .record_link_traversal(id, link.clone(), block_is_present);

        let builder = self.response_builder(block_size);
        builder.add_link(id, link, block_is_present);

        if let Some(block) = block {
            builder.add_block(block);
            true
        } else {
            false
        }
    }

    /// Adds the given extension data to to the response.
    pub fn send_extension_data(&mut self, id: RequestID, extension_data: ExtensionData) {
        // we pass 0 as the block size since we're not adding any blocks to the response
        self.response_builder(0)
            .add_extension_data(id, extension_data);
    }

    /// Marks the given request ID as having sent all responses.
    pub fn finish_request(&mut self, id: RequestID) -> ResponseStatusCode {
        let status = if self.link_tracker.finish_request(id) {
            ResponseStatusCode::RequestCompletedFull
        } else {
            ResponseStatusCode::RequestCompletedPartial
        };
        self.response_builder(0).complete(id, status);
        status
    }

    /// Marks the given requestID as having terminated with an error.
    pub fn finish_request_with_error(&mut self, id: RequestID, status: ResponseStatusCode) {
        self.link_tracker.finish_request(id);
        self.response_builder(0).complete(id, status);
    }

    /// Marks the given request ID as paused.
    pub fn pause_request(&mut self, id: RequestID) {
        self.response_builder(0)
            .complete(id, ResponseStatusCode::RequestPaused);
    }

    /// Either returns the most recent response builder or creates a new one, depending
    /// on whether the most recent one has enough space left to store a block with the
    /// given size.
    fn response_builder(&mut self, block_size: usize) -> &mut ResponseBuilder {
        assert!(
            block_size <= MAX_BLOCK_SIZE,
            "the size of a single block may not exceed the max block size"
        );

        match self.response_builders.last_mut() {
            Some(builder) if builder.block_size() + block_size <= MAX_BLOCK_SIZE => {}
            _ => self.response_builders.push(ResponseBuilder::new()),
        }
        self.response_builders.last_mut().unwrap()
    }

    /// Builds all responses and passes them to the given handler.
    pub async fn flush<H>(&mut self, handler: &mut H) -> Result<(), String>
    where
        H: PeerMessageHandler,
    {
        for builder in self.response_builders.drain(..) {
            let (responses, blocks) = builder.build()?;
            handler.send_response(&self.peer, responses, blocks).await;
        }
        Ok(())
    }
}

#[async_trait]
pub trait PeerMessageHandler {
    async fn send_response(
        &mut self,
        peer: &PeerId,
        responses: Vec<GraphSyncResponse>,
        blocks: Vec<Vec<u8>>,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils;

    struct Handler(Vec<(Vec<GraphSyncResponse>, Vec<Vec<u8>>)>);

    impl Handler {
        fn new() -> Self {
            Self(Vec::new())
        }

        fn take(&mut self) -> Vec<(Vec<GraphSyncResponse>, Vec<Vec<u8>>)> {
            std::mem::take(&mut self.0)
        }
    }

    #[async_trait]
    impl PeerMessageHandler for Handler {
        async fn send_response(
            &mut self,
            _peer: &PeerId,
            responses: Vec<GraphSyncResponse>,
            blocks: Vec<Vec<u8>>,
        ) {
            self.0.push((responses, blocks));
        }
    }

    #[async_std::test]
    async fn send_responses() {
        let peer = PeerId::random();
        let mut sender = PeerResponseSender::new(peer);
        let mut handler = Handler::new();

        let request_ids = [0, 1, 2];
        let (data, links) = test_utils::random_blocks(5, 100);

        let is_sent = sender.send_response(request_ids[0], links[0].clone(), Some(data[0].clone()));
        assert!(is_sent);

        sender.flush(&mut handler).await.unwrap();

        let mut messages = handler.take();
        assert_eq!(messages.len(), 1);
        let (responses, blocks) = messages.remove(0);

        assert_eq!(responses.len(), 1);
        assert_eq!(responses[0].id, request_ids[0]);
        assert_eq!(responses[0].status, ResponseStatusCode::PartialResponse);

        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0], data[0]);

        // we traverse the same block as part of a different request while the first request
        // is still in progress, so this one should not be sent
        let is_sent = sender.send_response(request_ids[1], links[0].clone(), Some(data[0].clone()));
        assert!(!is_sent);

        let is_sent = sender.send_response(request_ids[0], links[1].clone(), Some(data[1].clone()));
        assert!(is_sent);

        let is_sent = sender.send_response(request_ids[0], links[2].clone(), None);
        assert!(!is_sent);

        sender.finish_request(request_ids[0]);
        sender.flush(&mut handler).await.unwrap();

        let mut messages = handler.take();
        assert_eq!(messages.len(), 1);
        let (mut responses, blocks) = messages.remove(0);

        assert_eq!(responses.len(), 2);
        responses.sort_by_key(|r| r.id);
        assert_eq!(responses[0].id, request_ids[0]);
        assert_eq!(
            responses[0].status,
            ResponseStatusCode::RequestCompletedPartial
        );
        assert_eq!(responses[1].id, request_ids[1]);
        assert_eq!(responses[1].status, ResponseStatusCode::PartialResponse);

        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0], data[1]);

        let is_sent = sender.send_response(request_ids[1], links[3].clone(), Some(data[3].clone()));
        assert!(is_sent);

        let is_sent = sender.send_response(request_ids[2], links[4].clone(), Some(data[4].clone()));
        assert!(is_sent);

        sender.finish_request(request_ids[1]);
        sender.flush(&mut handler).await.unwrap();

        let mut messages = handler.take();
        assert_eq!(messages.len(), 1);
        let (mut responses, blocks) = messages.remove(0);

        assert_eq!(responses.len(), 2);
        responses.sort_by_key(|r| r.id);
        assert_eq!(responses[0].id, request_ids[1]);
        assert_eq!(
            responses[0].status,
            ResponseStatusCode::RequestCompletedFull
        );
        assert_eq!(responses[1].id, request_ids[2]);
        assert_eq!(responses[1].status, ResponseStatusCode::PartialResponse);

        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0], data[3]);
        assert_eq!(blocks[1], data[4]);

        // this block has already been sent to the peer but that request has already
        // been completed
        let is_sent = sender.send_response(request_ids[2], links[0].clone(), Some(data[0].clone()));
        assert!(is_sent);

        // this block has already been sent to the peer, as part of the same request
        let is_sent = sender.send_response(request_ids[2], links[4].clone(), Some(data[4].clone()));
        assert!(!is_sent);

        sender.flush(&mut handler).await.unwrap();

        let mut messages = handler.take();
        assert_eq!(messages.len(), 1);
        let (responses, blocks) = messages.remove(0);

        assert_eq!(responses.len(), 1);
        assert_eq!(responses[0].id, request_ids[2]);
        assert_eq!(responses[0].status, ResponseStatusCode::PartialResponse);

        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0], data[0]);
    }

    #[async_std::test]
    async fn send_large_responses() {
        let peer = PeerId::random();
        let mut sender = PeerResponseSender::new(peer);
        let mut handler = Handler::new();

        let request_id = 0;
        // just below the 512kb maximum block size, so each block is put in a separate message
        let (data, links) = test_utils::random_blocks(5, 500_000);

        sender.send_response(request_id, links[0].clone(), Some(data[0].clone()));
        sender.flush(&mut handler).await.unwrap();

        let mut messages = handler.take();
        assert_eq!(messages.len(), 1);
        let (responses, _) = messages.remove(0);

        assert_eq!(responses.len(), 1);
        assert_eq!(responses[0].status, ResponseStatusCode::PartialResponse);

        for i in 1..=4 {
            sender.send_response(request_id, links[i].clone(), Some(data[i].clone()));
        }
        sender.finish_request(request_id);
        sender.flush(&mut handler).await.unwrap();

        let messages = handler.take();
        assert_eq!(messages.len(), 4);

        for (i, (responses, blocks)) in (1..=4).zip(messages) {
            let status = match i {
                4 => ResponseStatusCode::RequestCompletedFull,
                _ => ResponseStatusCode::PartialResponse,
            };

            assert_eq!(responses.len(), 1);
            assert_eq!(responses[0].status, status);

            assert_eq!(blocks.len(), 1);
            assert_eq!(blocks[0], data[i]);
        }
    }

    #[async_std::test]
    async fn send_extension_data() {
        let peer = PeerId::random();
        let mut sender = PeerResponseSender::new(peer);
        let mut handler = Handler::new();

        let request_id = 0;
        let (data, links) = test_utils::random_blocks(2, 100);

        sender.send_response(request_id, links[0].clone(), Some(data[0].clone()));
        sender.flush(&mut handler).await.unwrap();

        let mut messages = handler.take();
        assert_eq!(messages.len(), 1);
        let (responses, blocks) = messages.remove(0);

        assert_eq!(responses.len(), 1);
        assert_eq!(responses[0].status, ResponseStatusCode::PartialResponse);

        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0], data[0]);

        let extension1 = ExtensionData {
            name: "AppleSauce/McGee".to_string(),
            data: test_utils::random_bytes(100),
        };

        let extension2 = ExtensionData {
            name: "HappyLand/Happenstance".to_string(),
            data: test_utils::random_bytes(100),
        };

        sender.send_response(request_id, links[1].clone(), Some(data[1].clone()));
        sender.send_extension_data(request_id, extension1.clone());
        sender.send_extension_data(request_id, extension2.clone());
        sender.flush(&mut handler).await.unwrap();

        let mut messages = handler.take();
        assert_eq!(messages.len(), 1);
        let (responses, _) = messages.remove(0);
        assert_eq!(responses.len(), 1);

        assert_eq!(responses[0].extensions[&extension1.name], extension1.data);
        assert_eq!(responses[0].extensions[&extension2.name], extension2.data);
    }
}
