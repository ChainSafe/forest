// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#![allow(dead_code)]

mod link_tracker;
mod peer_response_sender;
mod response_builder;

use link_tracker::LinkTracker;
use peer_response_sender::{PeerMessageHandler, PeerResponseSender};
use response_builder::ResponseBuilder;

use super::{
    Extensions, GraphSyncRequest, NewRequestPayload, Payload, RequestID, ResponseStatusCode,
};
use async_trait::async_trait;
use cid::Cid;
use forest_ipld::{selector::LinkResolver, Ipld};
use ipld_blockstore::BlockStore;
use libp2p::core::PeerId;
use std::{collections::HashMap, sync::Arc};

/// Handles incoming graphsync requests from the network, initiates selector traversals, and transmits responses.
pub struct ResponseManager {
    peer_response_senders: HashMap<PeerId, PeerResponseSender>,
}

impl ResponseManager {
    /// Creates a new response manager.
    pub fn new() -> Self {
        Self {
            peer_response_senders: HashMap::new(),
        }
    }

    /// Returns the response sender associated with the given peer.
    fn sender_for_peer(&mut self, peer: PeerId) -> &mut PeerResponseSender {
        self.peer_response_senders
            .entry(peer.clone())
            .or_insert_with(|| PeerResponseSender::new(peer))
    }

    /// Executes the given request.
    pub async fn execute_request<L, H>(
        &mut self,
        peer: PeerId,
        request: GraphSyncRequest,
        loader: L,
        handler: &mut H,
    ) -> Result<(), String>
    where
        L: LinkResolver + Send + Sync,
        H: PeerMessageHandler,
    {
        match request.payload {
            Payload::New(payload) => {
                self.new_request(peer, request.id, payload, loader, handler)
                    .await
            }
            Payload::Update { extensions } => self.update_request(request.id, extensions).await,
            Payload::Cancel => self.cancel_request(request.id).await,
        }
    }

    /// Executes a new request.
    async fn new_request<L, H>(
        &mut self,
        peer_id: PeerId,
        request_id: RequestID,
        payload: NewRequestPayload,
        mut loader: L,
        handler: &mut H,
    ) -> Result<(), String>
    where
        L: LinkResolver + Send + Sync,
        H: PeerMessageHandler,
    {
        // TODO: look for the do-not-send-cids extension
        let NewRequestPayload { root, selector, .. } = payload;
        let sender = self.sender_for_peer(peer_id);

        let ipld: Ipld = match loader.load_link(&root).await? {
            Some(ipld) => ipld,
            None => {
                sender.finish_request_with_error(
                    request_id,
                    ResponseStatusCode::RequestFailedContentNotFound,
                );
                return sender.flush(handler).await;
            }
        };

        let intercepted = InterceptedLoader::new(loader, |cid, block| {
            let data = block
                .map(|ipld| forest_encoding::to_vec(ipld))
                .transpose()
                .map_err(|e| e.to_string())?;
            sender.send_response(request_id, cid.clone(), data);
            Ok(())
        });

        // we ignore the callback parameters because we're only interested in the
        // loaded blocks, which the intercepted loader takes care of
        selector
            .walk_all(&ipld, Some(intercepted), |_, _, _| Ok(()))
            .await
            .map_err(|e| e.to_string())?;
        sender.flush(handler).await
    }

    /// Updates an ongoing request.
    async fn update_request(
        &mut self,
        _id: RequestID,
        _extensions: Extensions,
    ) -> Result<(), String> {
        // we can't implement this yet because requests are currently executed in one go
        todo!()
    }

    /// Cancels an ongoing request.
    async fn cancel_request(&mut self, _id: RequestID) -> Result<(), String> {
        // we can't implement this yet because requests are currently executed in one go
        todo!()
    }
}

/// A block loader that wraps another loader and calls a callback whenever
/// a link is loaded with the cid and the corresponding block.
struct InterceptedLoader<L, F> {
    loader: L,
    f: F,
}

impl<L, F> InterceptedLoader<L, F>
where
    L: LinkResolver + Send + Sync,
    F: FnMut(&Cid, Option<&Ipld>) -> Result<(), String> + Send + Sync,
{
    fn new(loader: L, f: F) -> Self {
        Self { loader, f }
    }
}

#[async_trait]
impl<L, F> LinkResolver for InterceptedLoader<L, F>
where
    L: LinkResolver + Send + Sync,
    F: FnMut(&Cid, Option<&Ipld>) -> Result<(), String> + Send + Sync,
{
    async fn load_link(&mut self, link: &Cid) -> Result<Option<Ipld>, String> {
        let ipld = self.loader.load_link(link).await?;
        (self.f)(link, ipld.as_ref())?;
        Ok(ipld)
    }
}

/// A block loader that loads the blocks from a blockstore.
// TODO: put this type somewhere else, graphsync doesn't need to know about blockstores
struct BlockStoreLoader<BS> {
    blockstore: Arc<BS>,
}

#[async_trait]
impl<BS> LinkResolver for BlockStoreLoader<BS>
where
    BS: BlockStore + Send + Sync,
{
    async fn load_link(&mut self, link: &Cid) -> Result<Option<Ipld>, String> {
        self.blockstore.get(link).map_err(|e| e.to_string())
    }
}
