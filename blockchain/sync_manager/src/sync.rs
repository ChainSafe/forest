// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

use super::errors::Error;
use super::manager::SyncManager;
use blocks::{Tipset, Block};
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
    /// InformNewHead informs the syncer about a new potential tipset
    /// This should be called when connecting to new peers, and additionally
    /// when receiving new blocks from the network
    fn inform_new_head(&self, from: PeerId, full_block: Vec<Block>) {
        // check if full block is nil and if so return error
        if full_block.is_empty() {
            return Err(Error::NoBlocks);
        }
        for b in 0..full_block {
            // TODO
            // validate message data and store in MessageStore
        }

        // TODO
        // send pubsub message indicating incoming blocks
        // self.incoming.PubsubMessage{topics: "incoming", message: full_block}

        // TODO
        // Store incoming block header

        // TODO
        // Add peer to blocksync

        // compare targetweight to heaviest weight stored
        // ignore otherwise

        // set peer head
        self.sync_manager.set_peer_head(from, )

    }

    fn validate_msg_data(&self, block: Block) {
        // TODO
        unimplemented!()
    }
}