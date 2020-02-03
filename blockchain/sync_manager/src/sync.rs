// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#![allow(dead_code)]

use super::errors::Error;
use super::manager::SyncManager;
use blocks::{Block, FullTipset, Tipset};
use chain::ChainStore;
use cid::{Cid, Codec, Error as CidError, Version};
use libp2p::core::PeerId;
use multihash::Multihash;
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
        // validate message data
        for block in fts.blocks() {
            self.validate_msg_data(block)?;
        }
        // TODO send pubsub message indicating incoming blocks
        // TODO Add peer to blocksync

        // compare target_weight to heaviest weight stored; ignore otherwise
        let best_weight = self.chain_store.heaviest_tipset().blocks()[0].weight();
        let target_weight = fts.blocks()[0].to_header().weight();

        if !target_weight.lt(&best_weight) {
            // Store incoming block header
            self.chain_store.persist_headers(&fts.tipset()?)?;
            // Set peer head
            self.sync_manager.set_peer_head(from, fts.tipset()?);
        }
        // incoming tipset from miners does not appear to be better than our best chain, ignoring for now
        Ok(())
    }

    fn validate_msg_data(&self, block: &Block) -> Result<(), Error> {
        let sm_root = self.compute_msg_data(block)?;
        // TODO change message_receipts to messages() once #192 is in
        if block.to_header().message_receipts() != &sm_root {
            return Err(Error::InvalidRoots);
        }

        self.chain_store.put_messages(block.bls_msgs())?;
        self.chain_store.put_messages(block.secp_msgs())?;

        Ok(())
    }
    fn compute_msg_data(&self, block: &Block) -> Result<Cid, CidError> {
        // TODO compute message roots

        let _bls_cids = cids_from_messages(block.bls_msgs())?;
        let _secp_cids = cids_from_messages(block.secp_msgs())?;

        // TODO temporary until AMT structure is implemented
        // see Lotus implementation https://github.com/filecoin-project/lotus/blob/master/chain/sync.go#L338
        // will return a new CID representing both message roots
        let hash = Multihash::from_bytes(vec![0, 0]);
        Ok(Cid::new(Codec::DagCBOR, Version::V1, hash.unwrap()))
    }
}

pub fn cids_from_messages<T: RawBlock>(messages: &[T]) -> Result<Vec<Cid>, CidError> {
    messages.iter().map(RawBlock::cid).collect()
}
