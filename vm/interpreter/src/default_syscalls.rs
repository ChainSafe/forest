// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::resolve_to_key_addr;
use actor::miner;
use address::Address;
use blocks::BlockHeader;
use fil_types::{verifier::ProofVerifier, SealVerifyInfo, WindowPoStVerifyInfo};
use forest_encoding::from_slice;
use ipld_blockstore::BlockStore;
use log::warn;
use rayon::prelude::*;
use runtime::{ConsensusFault, ConsensusFaultType, Syscalls};
use state_tree::StateTree;
use std::marker::PhantomData;
use std::{collections::HashMap, error::Error as StdError};

/// Default syscalls information
pub struct DefaultSyscalls<'bs, BS, V> {
    store: &'bs BS,
    verifier: PhantomData<V>,
}

impl<'bs, BS, V> DefaultSyscalls<'bs, BS, V> {
    pub fn new(store: &'bs BS) -> Self {
        Self {
            store,
            verifier: Default::default(),
        }
    }
}

impl<'bs, BS, V> Syscalls for DefaultSyscalls<'bs, BS, V>
where
    BS: BlockStore,
    V: ProofVerifier,
{
    fn verify_consensus_fault(
        &self,
        h1: &[u8],
        h2: &[u8],
        extra: &[u8],
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

    fn verify_seal(&self, vi: &SealVerifyInfo) -> Result<(), Box<dyn StdError>> {
        V::verify_seal(vi)
    }

    fn verify_post(
        &self,
        WindowPoStVerifyInfo {
            randomness,
            proofs,
            challenged_sectors,
            prover,
        }: &WindowPoStVerifyInfo,
    ) -> Result<(), Box<dyn StdError>> {
        V::verify_window_post(*randomness, &proofs, challenged_sectors, *prover)
    }

    fn batch_verify_seals(
        &self,
        vis: &[(Address, &Vec<SealVerifyInfo>)],
    ) -> Result<HashMap<Address, Vec<bool>>, Box<dyn StdError>> {
        // TODO ideal to not use rayon https://github.com/ChainSafe/forest/issues/676
        let out = vis
            .par_iter()
            .map(|(addr, seals)| {
                let results = seals
                    .par_iter()
                    .map(|s| {
                        if let Err(err) = V::verify_seal(s) {
                            warn!(
                                "seal verify in batch failed (miner: {}) (err: {})",
                                addr, err
                            );
                            false
                        } else {
                            true
                        }
                    })
                    .collect();
                (*addr, results)
            })
            .collect();
        Ok(out)
    }
}

impl<'bs, BS, V> DefaultSyscalls<'bs, BS, V>
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

        let info = ms.get_info(self.store)?;
        let work_address = resolve_to_key_addr(&state, self.store, &info.worker)?;
        bh.check_block_signature(&work_address)?;
        Ok(())
    }
}
