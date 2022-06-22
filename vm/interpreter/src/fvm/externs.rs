// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use crate::gas_block_store::GasBlockStore;
use crate::price_list_by_epoch;
use crate::GasTracker;
use crate::Rand;
use cid::Cid;
use clock::ChainEpoch;
use fil_actors_runtime::runtime::Policy;
use fvm::externs::Consensus;
use fvm::externs::Externs;
use fvm_shared::consensus::{ConsensusFault, ConsensusFaultType};
use ipld_blockstore::BlockStore;

use crate::resolve_to_key_addr;
use address::Address;
use blocks::BlockHeader;
use forest_encoding::Cbor;
use state_tree::StateTree;

use anyhow::bail;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

pub struct ForestExterns<DB> {
    rand: Box<dyn Rand>,
    epoch: ChainEpoch,
    calico_height: ChainEpoch,
    root: Cid,
    lookback: Box<dyn Fn(ChainEpoch) -> Cid>,
    db: Arc<DB>,
}

impl<DB: BlockStore> ForestExterns<DB> {
    pub fn new(
        rand: impl Rand + 'static,
        epoch: ChainEpoch,
        calico_height: ChainEpoch,
        root: Cid,
        lookback: Box<dyn Fn(ChainEpoch) -> Cid>,
        db: Arc<DB>,
    ) -> Self {
        ForestExterns {
            rand: Box::new(rand),
            epoch,
            calico_height,
            root,
            lookback,
            db,
        }
    }

    fn worker_key_at_lookback(
        &self,
        miner_addr: &Address,
        height: ChainEpoch,
    ) -> anyhow::Result<(Address, i64)> {
        if height < self.epoch - Policy::default().chain_finality {
            bail!(
                "cannot get worker key (current epoch: {}, height: {})",
                self.epoch,
                height
            );
        }

        let prev_root = (self.lookback)(height);
        let lb_state = StateTree::new_from_root(self.db.as_ref(), &prev_root)
            .map_err(|e| anyhow::anyhow!("{}", e))?;

        let actor = lb_state
            .get_actor(miner_addr)
            .map_err(|e| anyhow::anyhow!("{}", e))?
            .ok_or_else(|| anyhow::anyhow!("actor not found {:?}", miner_addr))?;

        let tracker = Rc::new(RefCell::new(GasTracker::new(i64::MAX, 0)));
        let gbs = GasBlockStore {
            price_list: price_list_by_epoch(self.epoch, self.calico_height),
            gas: tracker.clone(),
            store: self.db.as_ref(),
        };

        let ms = actor::miner::State::load(&gbs, &actor).map_err(|e| anyhow::anyhow!("{}", e))?;

        let worker = ms.info(&gbs).map_err(|e| anyhow::anyhow!("{}", e))?.worker;

        let state = StateTree::new_from_root(self.db.as_ref(), &self.root)
            .map_err(|e| anyhow::anyhow!("{}", e))?;

        let addr =
            resolve_to_key_addr(&state, &gbs, &worker).map_err(|e| anyhow::anyhow!("{}", e))?;

        let gas_used = tracker.borrow().gas_used();
        Ok((addr, gas_used))
    }

    fn verify_block_signature(&self, bh: &BlockHeader) -> anyhow::Result<i64> {
        let (worker_addr, gas_used) =
            self.worker_key_at_lookback(bh.miner_address(), bh.epoch())?;

        bh.check_block_signature(&worker_addr)?;

        Ok(gas_used)
    }
}

impl<DB: BlockStore> Externs for ForestExterns<DB> {}

impl<DB> Rand for ForestExterns<DB> {
    fn get_chain_randomness(
        &self,
        pers: i64,
        round: ChainEpoch,
        entropy: &[u8],
    ) -> anyhow::Result<[u8; 32]> {
        self.rand.get_chain_randomness(pers, round, entropy)
    }

    fn get_beacon_randomness(
        &self,
        pers: i64,
        round: ChainEpoch,
        entropy: &[u8],
    ) -> anyhow::Result<[u8; 32]> {
        self.rand.get_beacon_randomness(pers, round, entropy)
    }
}

impl<DB: BlockStore> Consensus for ForestExterns<DB> {
    fn verify_consensus_fault(
        &self,
        h1: &[u8],
        h2: &[u8],
        extra: &[u8],
    ) -> anyhow::Result<(Option<ConsensusFault>, i64)> {
        let mut total_gas: i64 = 0;

        // Note that block syntax is not validated. Any validly signed block will be accepted pursuant to the below conditions.
        // Whether or not it could ever have been accepted in a chain is not checked/does not matter here.
        // for that reason when checking block parent relationships, rather than instantiating a Tipset to do so
        // (which runs a syntactic check), we do it directly on the CIDs.

        // (0) cheap preliminary checks

        // are blocks the same?
        if h1 == h2 {
            bail!(
                "no consensus fault: submitted blocks are the same: {:?}, {:?}",
                h1,
                h2
            );
        };
        let bh_1 = BlockHeader::unmarshal_cbor(h1)?;
        let bh_2 = BlockHeader::unmarshal_cbor(h2)?;

        if bh_1.cid() == bh_2.cid() {
            bail!("no consensus fault: submitted blocks are the same");
        }

        // (1) check conditions necessary to any consensus fault

        if bh_1.miner_address() != bh_2.miner_address() {
            bail!(
                "no consensus fault: blocks not mined by same miner: {:?}, {:?}",
                bh_1.miner_address(),
                bh_2.miner_address()
            );
        };
        // block a must be earlier or equal to block b, epoch wise (ie at least as early in the chain).
        if bh_2.epoch() < bh_1.epoch() {
            bail!(
                "first block must not be of higher height than second: {:?}, {:?}",
                bh_1.epoch(),
                bh_2.epoch()
            );
        };

        // (2) check for the consensus faults themselves
        let mut cf: Option<ConsensusFault> = None;

        // (a) double-fork mining fault
        if bh_1.epoch() == bh_2.epoch() {
            cf = Some(ConsensusFault {
                target: *bh_1.miner_address(),
                epoch: bh_2.epoch(),
                fault_type: ConsensusFaultType::DoubleForkMining,
            })
        };

        // (b) time-offset mining fault
        // strictly speaking no need to compare heights based on double fork mining check above,
        // but at same height this would be a different fault.
        if bh_1.parents() == bh_2.parents() && bh_1.epoch() != bh_2.epoch() {
            cf = Some(ConsensusFault {
                target: *bh_1.miner_address(),
                epoch: bh_2.epoch(),
                fault_type: ConsensusFaultType::TimeOffsetMining,
            })
        };

        // (c) parent-grinding fault
        // Here extra is the "witness", a third block that shows the connection between A and B as
        // A's sibling and B's parent.
        // Specifically, since A is of lower height, it must be that B was mined omitting A from its tipset
        if !extra.is_empty() {
            let bh_3 = BlockHeader::unmarshal_cbor(extra)?;
            if bh_1.parents() == bh_3.parents()
                && bh_1.epoch() == bh_3.epoch()
                && bh_2.parents().cids().contains(bh_3.cid())
                && !bh_2.parents().cids().contains(bh_1.cid())
            {
                cf = Some(ConsensusFault {
                    target: *bh_1.miner_address(),
                    epoch: bh_2.epoch(),
                    fault_type: ConsensusFaultType::ParentGrinding,
                })
            }
        };

        // (3) return if no consensus fault
        if cf.is_none() {
            return Ok((cf, total_gas));
        }

        // (4) expensive final checks

        // check blocks are properly signed by their respective miner
        // note we do not need to check extra's: it is a parent to block b
        // which itself is signed, so it was willingly included by the miner
        total_gas += self.verify_block_signature(&bh_1)?;
        total_gas += self.verify_block_signature(&bh_2)?;

        Ok((cf, total_gas))
    }
}
