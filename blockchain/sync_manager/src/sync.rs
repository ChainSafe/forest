// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

#![allow(dead_code)]

use super::errors::Error;
use super::manager::SyncManager;
use blocks::{Block, FullTipset, Tipset};
use chain::ChainStore;
use cid::Cid;
use libp2p::core::PeerId;
use raw_block::RawBlock;

pub struct Syncer<'a> {
    // TODO add ability to send msg to all subscribers indicating incoming blocks
    // TODO add state manager
    // TODO add block sync

    // manages sync buckets
    sync_manager: SyncManager<'a>,
    // access and store tipsets / blocks / messages
    chain_store: ChainStore<'a>,
    // the known genesis tipset
    _genesis: Tipset,
    // self peerId
    _own: PeerId,
}

impl<'a> Syncer<'a> {
    /// TODO add constructor

    /// informs the syncer about a new potential tipset
    /// This should be called when connecting to new peers, and additionally
    /// when receiving new blocks from the network
    fn inform_new_head(&self, from: PeerId, fts: FullTipset) -> Result<(), Error> {
        // check if full block is nil and if so return error
        if fts.blocks().is_empty() {
            return Err(Error::NoBlocks);
        }
        // TODO validate message data
        for block in fts.blocks() {
            self.validate_msg_data(block);
        }
        // TODO send pubsub message indicating incoming blocks
        // TODO Add peer to blocksync

        // compare targetweight to heaviest weight stored; ignore otherwise
        let best_weight = self.chain_store.get_heaviest_tipset().blocks()[0].weight();
        let target_weight = fts.tipset()?.blocks()[0].weight();
        if !target_weight.lt(&best_weight) {
            // Store incoming block header
            self.chain_store.persist_headers(&fts.tipset()?).ok();
            // Set peer head
            self.sync_manager.set_peer_head(from, fts.tipset()?);
        }
        // incoming tipset from miners does not appear to be better than our best chain, ignoring for now
        Ok(())
    }

    fn validate_msg_data(&self, block: &Block) -> Result<(), Error> {
        let sm_root = self.compute_msg_data(block);
        // TODO change message_receipts to messages() once #192 is in
        if block.to_header().message_receipts() != sm_root {
            return Err(Error::InvalidRoots);
        }

        for b in block.get_bls_msgs() {
            // store in datastore
            self.chain_store.put_messages(b.cid().key(), b.raw_data()?);
        }
        for b in block.get_secp_msgs() {
            // store in datastore
            self.chain_store.put_messages(b.cid().key(), b.raw_data()?);
        }

        Ok(())
    }
    fn compute_msg_data(&self, block: &Block) -> &Cid {
        // TODO compute message roots
       
        let mut bls_cids = Vec::new();
        let mut secp_cids = Vec::new();

        for b in block.get_bls_msgs() {
            bls_cids.push(b.cid());
        }
        for b in block.get_secp_msgs() {
            secp_cids.push(b.cid());
        }
        // Temporary until AMT structure is implemented
        &block.to_header().cid()
    }
}
