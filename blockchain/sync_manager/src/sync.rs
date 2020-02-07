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
use std::time::{SystemTime, UNIX_EPOCH};

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

struct MsgMetaData {
    balance: BigUint,
    sequence: u64,
}

struct MsgCheck {
    metadata: HashMap<Address, MsgMetaData>,
}

impl MsgCheck {
    /// Creates new MsgCheck with empty metadata
    pub fn new() -> Self {
        Self {
            metadata: HashMap::new(),
        }
    }
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
    fn check_blk_msgs(&self, block: Block, _tip: Tipset) -> Result<(), Error> {
        for _m in block.bls_msgs() {
            // TODO retrieve bls public keys for verify_bls_aggregate
        }
        // TODO verify_bls_aggregate

        // check msgs for validity
        fn check_msg<T: Message>(
            msg: T,
            msg_meta_data: &mut MsgCheck,
            tree: &HamtStateTree,
        ) -> Result<(), Error>
        where
            T: Message,
        {
            let updated_state: MsgMetaData = match msg_meta_data.metadata.get(msg.from()) {
                // address is present begin validity checks
                Some(MsgMetaData { sequence, balance }) => {
                    // sequence equality check
                    if *sequence != msg.sequence() {
                        return Err(Error::Message("Sequences are not equal".to_string()));
                    }

                    // sufficient funds check
                    if balance < &msg.required_funds() {
                        return Err(Error::Message("Insufficient funds".to_string()));
                    }
                    // update balance and increment sequence by 1
                    MsgMetaData {
                        balance: balance - msg.required_funds(),
                        sequence: sequence + 1,
                    }
                }
                // sequence is not found, insert sequence and balance with address as key
                _ => {
                    let actor = tree.get_actor(msg.from());
                    if let Some(act) = actor {
                        MsgMetaData {
                            sequence: *act.sequence(),
                            balance: act.balance().clone(),
                        }
                    } else {
                        return Err(Error::Message("Sequences are not equal".to_string()));
                    }
                }
            };
            msg_meta_data
                .metadata
                .insert(msg.from().clone(), updated_state);
            Ok(())
        }

        let mut msg_meta_data = MsgCheck::new();
        // TODO retrieve tipset state and load state tree
        // temporary
        let tree = HamtStateTree::default();
        // loop through bls messages and check msg validity
        for m in block.bls_msgs() {
            check_msg(m.clone(), &mut msg_meta_data, &tree)?;
        }
        // loop through secp messages and check msg validity and signature
        for m in block.secp_msgs() {
            check_msg(m.clone(), &mut msg_meta_data, &tree)?;
            // signature validation
            if !is_valid_signature(&m.cid()?.to_bytes(), m.from(), m.signature()) {
                return Err(Error::Message("Message signature is not valid".to_string()));
            }
        }
        let secp_cids = cids_from_messages(block.secp_msgs())?;
        let bls_cids = cids_from_messages(block.bls_msgs())?;
        let bls_root = AMT::new_from_slice(self.chain_store.blockstore(), &bls_cids)?;
        let secp_root = AMT::new_from_slice(self.chain_store.blockstore(), &secp_cids)?;

        let meta = MsgMeta {
            bls_message_root: bls_root,
            secp_message_root: secp_root,
        };
        // store message roots and receive meta_root
        self.chain_store.blockstore().put(&meta)?;

        Ok(())
    }

    /// Should match up with 'Semantical Validation' in validation.md in the spec
    pub fn validate(&self, block: Block) -> Result<(), Error> {
        /* TODO block validation essentially involves 7 main checks:
            1. time_check: Must have a valid timestamp
            2. winner_check: Must verify it contains the winning ticket
            3. message_check: All messages in the block must be valid
            4. miner_check: Must be from a valid miner
            5. block_sig_check: Must have a valid signature by the miner address of the final ticket
            6. verify_ticket_vrf: Must be generated from the smallest ticket in the parent tipset and from same miner
            7. verify_election_proof_check: Must include an election proof which is a valid signature by the miner address of the final ticket
        */

        // get header from full block
        let header = block.to_header();
        let _base_tipset = self.load_fts(TipSetKeys::new((*header.parents().cids).to_vec()))?;

        // check if block has been signed
        if header.signature().bytes().is_empty() {
            return Err(Error::Blockchain("Signature is nil in header".to_string()));
        }

        /*
        1. time_check rules:
          - must include a timestamp not in the future
          - must have a valid timestamp
          - must be later than the earliest parent block time plus appropriate delay, which is BLOCK_DELAY
        */

        // Timestamp checks
        // TODO include allowable clock drift
        let time_now = match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(n) => n.as_secs(),
            Err(_) => {
                return Err(Error::Validation(
                    "SystemTime before UNIX EPOCH!".to_string(),
                ))
            }
        };
        if time_now < header.timestamp() {
            return Err(Error::Validation("Timestamp from future".to_string()));
        };
        const FIXED_BLOCK_DELAY: u64 = 45;
        // TODO add Sub trait to ChainEpoch type when it becomes u64 and re-work below for readability
        // if header.timestamp() < base_tipset.tipset()?.min_timestamp()?+FIXED_BLOCK_DELAY*(*header.epoch() - *base_tipset.tipset()?.tip_epoch()) {
        //     return Err(Error::Validation("Block was generated too soon".to_string()));
        // }

        /*
        TODO
        2. winner_check rules:
          - TODOs missing the following pieces of data to validate ticket winner
                - miner slashing
                - miner power storage
                - miner sector size
                - fn is_ticket_winner()
          - See lotus check here for more details: https://github.com/filecoin-project/lotus/blob/master/chain/sync.go#L522
        */

        /*
        TODO
        3. message_check rules:
            - All messages in the block must be valid
            - The execution of each message, in the order they are in the block,
             must produce a receipt matching the corresponding one in the receipt set of the block
            - The resulting state root after all messages are applied, must match the one in the block
           TODOs missing the following pieces of data to validate messages
           - check_block_messages -> see https://github.com/filecoin-project/lotus/blob/master/chain/sync.go#L705
        */

        /*
        TODO
        4. miner_check rules:
            - Ensure miner is valid; miner_is_valid -> see https://github.com/filecoin-project/lotus/blob/master/chain/sync.go#L460
        */

        /*
        TODO
        5. block_sig_check rules:
            - Must have a valid signature by the miner address of the final ticket

            TODOs missing the following pieces of data
            - check_block_sigs -> see https://github.com/filecoin-project/lotus/blob/master/chain/types/blockheader_cgo.go#L13
        */

        /*
        TODO
        6. verify_ticket_vrf rules:
            - the ticket must be generated from the smallest ticket in the parent tipset
            - all tickets in the ticket array must have been generated by the same miner
           TODOs
           - Complete verify_vrf -> see https://github.com/filecoin-project/lotus/blob/master/chain/gen/gen.go#L600
        */

        /*
        TODO
        7. verify_election_proof_check rules:
            - Must include an election proof which is a valid signature by the miner address of the final ticket
            TODOs
            - verify_election_proof -> see https://github.com/filecoin-project/lotus/blob/master/chain/sync.go#L650
        */

        Ok(())
    }
}

pub fn cids_from_messages<T: RawBlock>(messages: &[T]) -> Result<Vec<Cid>, CidError> {
    messages.iter().map(RawBlock::cid).collect()
}
