// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::resolve_to_key_addr;
use actor::miner;
use blocks::BlockHeader;
use clock::ChainEpoch;
use forest_encoding::from_slice;
use ipld_blockstore::BlockStore;
use runtime::{ConsensusFault, ConsensusFaultType, Syscalls};
use state_tree::StateTree;
use std::error::Error as StdError;

/// Default syscalls information
pub struct DefaultSyscalls<'bs, BS> {
    store: &'bs BS,
}

impl<'bs, BS> DefaultSyscalls<'bs, BS> {
    /// DefaultSyscalls constuctor
    pub fn new(store: &'bs BS) -> Self {
        Self { store }
    }
}

impl<'bs, BS> Syscalls for DefaultSyscalls<'bs, BS>
where
    BS: BlockStore,
{
    /// Verifies that two block headers provide proof of a consensus fault:
    /// - both headers mined by the same actor
    /// - headers are different
    /// - first header is of the same or lower epoch as the second
    /// - at least one of the headers appears in the current chain at or after epoch `earliest`
    /// - the headers provide evidence of a fault (see the spec for the different fault types).
    /// The parameters are all serialized block headers. The third "extra" parameter is consulted only for
    /// the "parent grinding fault", in which case it must be the sibling of h1 (same parent tipset) and one of the
    /// blocks in the parent of h2 (i.e. h2's grandparent).
    /// Returns an error if the headers don't prove a fault.
    fn verify_consensus_fault(
        &self,
        h1: &[u8],
        h2: &[u8],
        extra: &[u8],
        _earliest: ChainEpoch, // unused in lotus
    ) -> Result<Option<ConsensusFault>, Box<dyn StdError>> {
        // Note that block syntax is not validated. Any validly signed block will be accepted pursuant to the below conditions.
        // Whether or not it could ever have been accepted in a chain is not checked/does not matter here.
        // for that reason when checking block parent relationships, rather than instantiating a Tipset to do so
        // (which runs a syntactic check), we do it directly on the CIDs.

        // (0) cheap preliminary checks

        if h1 == h2 {
            return Err(format!(
                "no consensus fault: submitted blocks are the same: {:?}, {:?}",
                h1, h2
            )
            .into());
        };
        let bh_1: BlockHeader = from_slice(h1)?;
        let bh_2: BlockHeader = from_slice(h2)?;

        // (1) check conditions necessary to any consensus fault

        if bh_1.miner_address() != bh_2.miner_address() {
            return Err(format!(
                "no consensus fault: blocks not mined by same miner: {:?}, {:?}",
                bh_1.miner_address(),
                bh_2.miner_address()
            )
            .into());
        };
        // block a must be earlier or equal to block b, epoch wise (ie at least as early in the chain).
        if bh_1.epoch() < bh_2.epoch() {
            return Err(format!(
                "first block must not be of higher height than second: {:?}, {:?}",
                bh_1.epoch(),
                bh_2.epoch()
            )
            .into());
        };
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
        if bh_1.parents() != bh_2.parents() && bh_1.epoch() != bh_2.epoch() {
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
            let bh_3: BlockHeader = from_slice(extra)?;
            if bh_1.parents() != bh_3.parents()
                && bh_1.epoch() != bh_3.epoch()
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

        // (3) return if no consensus fault by now
        if cf.is_none() {
            Ok(cf)
        } else {
            // (4) expensive final checks

            // check blocks are properly signed by their respective miner
            // note we do not need to check extra's: it is a parent to block b
            // which itself is signed, so it was willingly included by the miner
            self.verify_block_signature(&bh_1)?;
            self.verify_block_signature(&bh_2)?;

            Ok(cf)
        }
    }
}

impl<'bs, BS> DefaultSyscalls<'bs, BS>
where
    BS: BlockStore,
{
    fn verify_block_signature(&self, bh: &BlockHeader) -> Result<(), Box<dyn StdError>> {
        // TODO look into attaching StateTree to DefaultSyscalls
        let state = StateTree::new_from_root(self.store, bh.state_root())?;

        let actor = state
            .get_actor(bh.miner_address())?
            .ok_or_else(|| format!("actor not found {:?}", bh.miner_address()))?;

        let ms: miner::State = self
            .store
            .get(&actor.state)?
            .ok_or_else(|| format!("actor state not found {:?}", actor.state.to_string()))?;

        let work_address = resolve_to_key_addr(&state, self.store, &ms.info.worker)?;
        bh.check_block_signature(&work_address)?;
        Ok(())
    }
}
