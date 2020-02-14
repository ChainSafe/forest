// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#![allow(dead_code)]

use super::errors::Error;
use super::manager::SyncManager;
use address::Address;
use amt::{BlockStore, AMT};
use blocks::{Block, FullTipset, TipSetKeys, Tipset, TxMeta};
use chain::ChainStore;
use cid::{Cid, Error as CidError};
use crypto::is_valid_signature;
use libp2p::core::PeerId;
use message::Message;
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

/// Message data used to ensure valid state transition
struct MsgMetaData {
    balance: BigUint,
    sequence: u64,
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
        let target_weight = fts.blocks()[0].header().weight();

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
        if block.header().messages() != &sm_root {
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

        let meta = TxMeta {
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
    // Block message validation checks
    pub fn check_blk_msgs(&self, block: Block, _tip: Tipset) -> Result<(), Error> {
        // TODO retrieve bls public keys for verify_bls_aggregate
        // for _m in block.bls_msgs() {
        // }
        // TODO verify_bls_aggregate

        // check msgs for validity
        fn check_msg<T: Message>(
            msg: &T,
            msg_meta_data: &mut HashMap<Address, MsgMetaData>,
            tree: &HamtStateTree,
        ) -> Result<(), Error>
        where
            T: Message,
        {
            let updated_state: MsgMetaData = match msg_meta_data.get(msg.from()) {
                // address is present begin validity checks
                Some(MsgMetaData { sequence, balance }) => {
                    // sequence equality check
                    if *sequence != msg.sequence() {
                        return Err(Error::Validation("Sequences are not equal".to_string()));
                    }

                    // sufficient funds check
                    if *balance < msg.required_funds() {
                        return Err(Error::Validation(
                            "Insufficient funds for message execution".to_string(),
                        ));
                    }
                    // update balance and increment sequence by 1
                    MsgMetaData {
                        balance: balance - msg.required_funds(),
                        sequence: sequence + 1,
                    }
                }
                // MsgMetaData not found with provided address key, insert sequence and balance with address as key
                None => {
                    let actor = tree.get_actor(msg.from()).ok_or_else(|| {
                        Error::State("Could not retrieve actor from state tree".to_owned())
                    })?;

                    MsgMetaData {
                        sequence: actor.sequence,
                        balance: actor.balance,
                    }
                }
            };
            // update hash map with updated state
            msg_meta_data.insert(msg.from().clone(), updated_state);
            Ok(())
        }
        let mut msg_meta_data: HashMap<Address, MsgMetaData> = HashMap::new();
        // TODO retrieve tipset state and load state tree
        // temporary
        let tree = HamtStateTree::default();
        // loop through bls messages and check msg validity
        for m in block.bls_msgs() {
            check_msg(m, &mut msg_meta_data, &tree)?;
        }
        // loop through secp messages and check msg validity and signature
        for m in block.secp_msgs() {
            check_msg(m, &mut msg_meta_data, &tree)?;
            // signature validation
            if !is_valid_signature(&m.cid()?.to_bytes(), m.from(), m.signature()) {
                return Err(Error::Validation(
                    "Message signature is not valid".to_string(),
                ));
            }
        }
        // validate message root from header matches message root
        let sm_root = self.compute_msg_data(&block)?;
        if block.header().messages() != &sm_root {
            return Err(Error::InvalidRoots);
        }

        Ok(())
    }

    /// Validates block semantically according to https://github.com/filecoin-project/specs/blob/6ab401c0b92efb6420c6e198ec387cf56dc86057/validation.md
    pub fn validate(&self, block: Block) -> Result<(), Error> {
        // get header from full block
        let header = block.header();

        // check if block has been signed
        if header.signature().bytes().is_empty() {
            return Err(Error::Validation("Signature is nil in header".to_string()));
        }

        let base_tipset = self.load_fts(TipSetKeys::new(header.parents().cids.clone()))?;
        // time stamp checks
        header.validate_timestamps(&base_tipset)?;

        // check messages to ensure valid state transitions
        self.check_blk_msgs(block.clone(), base_tipset.tipset()?)?;

        // block signature check
        // TODO need to pass in raw miner address; temp using header miner address
        // see https://github.com/filecoin-project/lotus/blob/master/chain/sync.go#L611
        header.check_block_signature(header.miner_address())?;

        // TODO winner_check
        // TODO miner_check
        // TODO verify_ticket_vrf
        // TODO verify_election_proof_check

        Ok(())
    }
}

pub fn cids_from_messages<T: RawBlock>(messages: &[T]) -> Result<Vec<Cid>, CidError> {
    messages.iter().map(RawBlock::cid).collect()
}
