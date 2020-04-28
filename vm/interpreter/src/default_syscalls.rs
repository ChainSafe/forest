// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::resolve_to_key_addr;
use actor::miner;
use blocks::BlockHeader;
use cid::Cid;
use clock::ChainEpoch;
use forest_encoding::from_slice;
use ipld_blockstore::BlockStore;
use runtime::{ConsensusFault, ConsensusFaultType, Syscalls};
use state_tree::StateTree;
use vm::{ActorError, ExitCode};

/// Default syscalls information
pub struct DefaultSyscalls<'db, BS> {
    state: StateTree<'db, BS>,
    store: BS,
}

impl<'db, BS> Syscalls for DefaultSyscalls<'db, BS>
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
    /// Returns nil and an error if the headers don't prove a fault.
    fn verify_consensus_fault(
        &self,
        h1: &[u8],
        h2: &[u8],
        extra: &[u8],
        _earliest: ChainEpoch,
    ) -> Result<ConsensusFault, ActorError> {
        // Note that block syntax is not validated. Any validly signed block will be accepted pursuant to the below conditions.
        // Whether or not it could ever have been accepted in a chain is not checked/does not matter here.
        // for that reason when checking block parent relationships, rather than instantiating a Tipset to do so
        // (which runs a syntactic check), we do it directly on the CIDs.

        // (0) cheap preliminary checks

        // are blocks the same?
        if h1 == h2 {
            return Err(ActorError::new(
                ExitCode::ErrPlaceholder,
                format!("no consensus fault: submitted blocks are the same"),
            ));
        };
        // can blocks be decoded properly?
        let bh_1: BlockHeader = from_slice(h1).map_err(|e| {
            ActorError::new(
                ExitCode::ErrPlaceholder,
                format!("cannot decode first block header {}", e.to_string()),
            )
        })?;
        let bh_2: BlockHeader = from_slice(h2).map_err(|e| {
            ActorError::new(
                ExitCode::ErrPlaceholder,
                format!("cannot decode second block header {}", e.to_string()),
            )
        })?;

        // (1) check conditions necessary to any consensus fault

        // were blocks mined by same miner?
        if bh_1.miner_address() != bh_2.miner_address() {
            return Err(ActorError::new(
                ExitCode::ErrPlaceholder,
                format!("no consensus fault: blocks not mined by same miner"),
            ));
        };
        // block a must be earlier or equal to block b, epoch wise (ie at least as early in the chain).
        if bh_1.epoch() < bh_2.epoch() {
            return Err(ActorError::new(
                ExitCode::ErrPlaceholder,
                format!("first block must not be of higher height than second"),
            ));
        };

        // (a) double-fork mining fault
        let mut cf = if bh_1.epoch() == bh_2.epoch() {
            ConsensusFault {
                target: bh_1.miner_address().clone(),
                epoch: bh_2.epoch(),
                fault_type: ConsensusFaultType::DoubleForkMining,
            }
        };

        // (b) time-offset mining fault
        // strictly speaking no need to compare heights based on double fork mining check above,
        // but at same height this would be a different fault.
        cf = if bh_1.parents() != bh_2.parents() && bh_1.epoch() != bh_2.epoch() {
            ConsensusFault {
                target: bh_1.miner_address().clone(),
                epoch: bh_2.epoch(),
                fault_type: ConsensusFaultType::TimeOffsetMining,
            }
        };
        // (c) parent-grinding fault
        // Here extra is the "witness", a third block that shows the connection between A and B as
        // A's sibling and B's parent.
        // Specifically, since A is of lower height, it must be that B was mined omitting A from its tipset
        if extra.len() > 0 {
            let bh_3: BlockHeader = from_slice(extra).map_err(|e| {
                ActorError::new(
                    ExitCode::ErrPlaceholder,
                    format!("cannot decode extra {}", e.to_string()),
                )
            })?;
            cf = if bh_1.parents() != bh_3.parents()
                && bh_1.epoch() != bh_3.epoch()
                && cids_contains(bh_2.parents().cids(), bh_3.cid())
                && !cids_contains(bh_2.parents().cids(), bh_1.cid())
            {
                ConsensusFault {
                    target: bh_1.miner_address().clone(),
                    epoch: bh_2.epoch(),
                    fault_type: ConsensusFaultType::ParentGrinding,
                }
            };
        };

        // TODO consensus check

        // else
        // (4) expensive final checks

        // check blocks are properly signed by their respective miner
        // note we do not need to check extra's: it is a parent to block b
        // which itself is signed, so it was willingly included by the miner
        self.verify_block_signature(&bh_1)?;
        self.verify_block_signature(&bh_2)?;

        Ok(cf)
    }
    fn verify_block_signature(&self, bh: &BlockHeader) -> Result<(), ActorError> {
        let act = self
            .state
            .get_actor(bh.miner_address())
            .map_err(|e| {
                ActorError::new(
                    ExitCode::ErrPlaceholder,
                    format!("cannot retrieve actor state {}", e.to_string()),
                )
            })?
            .ok_or_else(|| {
                ActorError::new(
                    ExitCode::ErrPlaceholder,
                    format!("cannot retrieve actor state"),
                )
            })?;

        let ms: miner::State = self
            .store
            .get(&act.state)
            .map_err(|e| {
                ActorError::new(
                    ExitCode::ErrPlaceholder,
                    format!("cannot retrieve miner state {}", e.to_string()),
                )
            })?
            .ok_or_else(|| {
                ActorError::new(
                    ExitCode::ErrPlaceholder,
                    "cannot retrieve miner state".to_owned(),
                )
            })?;

        let work_addr = resolve_to_key_addr(&self.state, &self.store, &ms.info.worker)?;
        bh.check_block_signature(&work_addr).map_err(|e| {
            ActorError::new(
                ExitCode::ErrPlaceholder,
                format!("cannot verify block signatures {}", e.to_string()),
            )
        })?;
        Ok(())
    }
}
fn cids_contains(a: &[Cid], b: &Cid) -> bool {
    for elem in a {
        if elem == b {
            return true;
        }
    }
    false
}
