// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

use super::errors::Error;
use super::manager::SyncManager;
use blocks::{Tipset, Block, FullTipset};
use libp2p::core::PeerId;
use chain::ChainStore;
use network::service::NetworkMessage;

#[derive(Default)]
pub struct Syncer {
    // manages sync buckets
    sync_manager: SyncManager,
    // access and store tipsets / blocks / messages
    chain_store: ChainStore,
    // the known genesis tipset
    genesis: Tipset,
    // self peerId
    own: PeerId,
    // publish message to to all subscribers indicating incoming blocks
    incoming: NetworkMessage
}

impl Syncer {
    /// informs the syncer about a new potential tipset
    /// This should be called when connecting to new peers, and additionally
    /// when receiving new blocks from the network
    fn inform_new_head(&self, from: PeerId, fts: FullTipset) {
        // check if full block is nil and if so return error
        if fts.blocks.is_empty() {
            return Err(Error::NoBlocks);
        }
        // TODO validate message data
        for i in 0..fts.blocks.len() {
            self.validate_msg_data(fts.blocks[i])
        }

        // TODO
        // send pubsub message indicating incoming blocks

        // Store incoming block header
        self.chain_store.persist_headers(fts.tipset());

        // TODO
        // Add peer to blocksync

        // compare targetweight to heaviest weight stored
        // ignore otherwise


        // set peer head
        self.sync_manager.set_peer_head(from, fts.tipset());

    }

    fn validate_msg_data(&self, block: Block) {
        // TODO call compute_msg_data to get message roots
        unimplemented!()
    }
    fn compute_msg_data(&self, block: Block) {
        //TODO 
        unimplemented!()
    }
    
}