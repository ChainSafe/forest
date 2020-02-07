// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#![allow(dead_code)]

use super::errors::Error;
use super::manager::SyncManager;
use address::Address;
use amt::{BlockStore, AMT};
use blocks::{Block, FullTipset, TipSetKeys, Tipset};
use chain::ChainStore;
use cid::{Cid, Error as CidError};
use crypto::is_valid_signature;
use libp2p::core::PeerId;
use message::{Message, MsgMeta};
use num_bigint::BigUint;
use raw_block::RawBlock;
use state::{HamtStateTree, StateTree};
use std::collections::HashMap;

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
    /// Validates message root from header matches message root generated from the
    /// bls and secp messages contained in the passed in block and stores them in a key-value store
    fn validate_msg_data(&self, block: &Block) -> Result<(), Error> {
        let sm_root = self.compute_msg_data(block)?;
        if block.to_header().messages() != &sm_root {
            return Err(Error::InvalidRoots);
        }

        self.chain_store.put_messages(block.bls_msgs())?;
        self.chain_store.put_messages(block.secp_msgs())?;

        Ok(())
    }
    /// Returns message root CID from bls and secp message contained in the param Block
    fn compute_msg_data(&self, block: &Block) -> Result<Cid, Error> {
        // collect bls and secp cids
        let bls_cids = cids_from_messages(block.bls_msgs())?;
        let secp_cids = cids_from_messages(block.secp_msgs())?;
        // generate AMT and batch set message values
        let bls_root = AMT::new_from_slice(self.chain_store.blockstore(), &bls_cids)?;
        let secp_root = AMT::new_from_slice(self.chain_store.blockstore(), &secp_cids)?;

        let meta = MsgMeta {
            bls_message_root: bls_root,
            secp_message_root: secp_root,
        };
        // store message roots and receive meta_root
        let meta_root = self.chain_store.blockstore().put(&meta)?;

        Ok(meta_root)
    }
    /// Returns FullTipset from store if TipSetKeys exist in key-value store otherwise requests FullTipset
    /// from block sync
    fn fetch_tipsets(&self, _peer_id: PeerId, tsk: TipSetKeys) -> Result<FullTipset, Error> {
        let fts = match self.load_fts(tsk) {
            Ok(fts) => fts,
            // TODO call into block sync to request FullTipset -> self.blocksync.get_full_tipset(_peer_id, tsk)
            Err(e) => return Err(e), // blocksync
        };
        Ok(fts)
    }
    /// Returns a reconstructed FullTipset from store if keys exist
    fn load_fts(&self, keys: TipSetKeys) -> Result<FullTipset, Error> {
        let mut blocks = Vec::new();
        // retrieve tipset from store based on passed in TipSetKeys
        let ts = self.chain_store.tipset(keys.tipset_keys())?;
        for header in ts.blocks() {
            // retrieve bls and secp messages from specified BlockHeader
            let (bls_msgs, secp_msgs) = self.chain_store.messages(&header)?;
            // construct a full block
            let full_block = Block {
                header: header.clone(),
                bls_messages: bls_msgs,
                secp_messages: secp_msgs,
            };
            // push vector of full blocks to build FullTipset
            blocks.push(full_block);
        }
        // construct FullTipset
        let fts = FullTipset::new(blocks);
        Ok(fts)
    }
    // Block message validation checks; see https://github.com/filecoin-project/lotus/blob/master/chain/sync.go#L706
    fn check_blk_msgs(&self, block: Block, tip: Tipset) -> Result<(), Error> {
        for _m in block.bls_msgs() {
            // TODO verify bls sigs 
            // if !is_valid_signature(&m.cid()?.to_bytes(), m.from(), m.signature()) {
            // }
        }
        // TODO verify_bls_aggregate

        let mut balance = HashMap::new();
        let mut sequence = HashMap::new();
        // TODO retrieve tipset state and load state tree
        // temporary
        let mut tree = HamtStateTree::default();

        // check msgs for validity
        fn checkMsg<T: Message>(
            msg: T,
            seq: &mut HashMap<&Address, u64>,
            bal: &mut HashMap<&Address, BigUint>,
            tree: HamtStateTree,
        ) -> Result<(), Error> 
        {
            match seq.get(&msg.from()) {
                // address is present begin validity checks
                Some(&addr) => {
                    // sequence equality check
                    if *seq
                        .get(msg.from())
                        .ok_or::<Error>(Err(Error::Message("Cannot retrieve sequence from address key".to_string()))?)
                        .unwrap()
                        != msg.sequence()
                    {
                        return Err(Error::Message("Sequences are not equal".to_string()));
                    }
                    // increment sequence by 1
                    *seq.get(msg.from())
                        .ok_or::<Error>(Err(Error::Message("Cannot retrieve sequence from address key".to_string()))?)
                        .unwrap()
                        + 1;
                    // sufficient funds check
                    if *bal
                        .get(msg.from())
                        .ok_or::<Error>(Err(Error::Message("Cannot retrieve balance from address key".to_string()))?)
                        .unwrap()
                        < msg.required_funds()
                    {
                        return Err(Error::Message("Insufficient funds".to_string()));
                    }
                    // update balance
                    let mut v = bal
                        .get(msg.from())
                        .ok_or::<Error>(Err(Error::Message("Cannot retrieve balance from address key".to_string()))?)
                        .unwrap();
                    bal.insert(msg.from(), *v - msg.required_funds());
                }
                // sequence is not found, insert sequence and balance with address as key
                _ => {
                    let act = tree
                        .get_actor(msg.from())
                        .ok_or::<Error>(Err(Error::State("Cannot retrieve actor from state".to_string()))?)
                        .unwrap();
                    seq.insert(msg.from(), *act.sequence());
                    bal.insert(msg.from(), *act.balance());
                }
            }
            Ok(())
        }
        let bls_cids = Vec::new();
        let secp_cids = Vec::new();
        // loop through bls messages and check msg validity
        for m in block.bls_msgs() {
            checkMsg(*m, &mut sequence, &mut balance, tree)?;
            bls_cids = cids_from_messages(block.bls_msgs())?;
        }
        // loop through secp messages and check msg validity and signature
        for m in block.secp_msgs() {
            checkMsg(*m, &mut sequence, &mut balance, tree)?;
            // signature validation
            if !is_valid_signature(&m.cid()?.to_bytes(), m.from(), m.signature()) {
                return Err(Error::Message("Message signature is not valid".to_string()));
            }
            secp_cids = cids_from_messages(block.secp_msgs())?;
        }
        let bls_root = AMT::new_from_slice(self.chain_store.blockstore(), &bls_cids)?;
        let secp_root = AMT::new_from_slice(self.chain_store.blockstore(), &secp_cids)?;

        let meta = MsgMeta {
            bls_message_root: bls_root,
            secp_message_root: secp_root,
        };
        // store message roots and receive meta_root
        let meta_root = self.chain_store.blockstore().put(&meta)?;

        Ok(())
    }
}

pub fn cids_from_messages<T: RawBlock>(messages: &[T]) -> Result<Vec<Cid>, CidError> {
    messages.iter().map(RawBlock::cid).collect()
}
