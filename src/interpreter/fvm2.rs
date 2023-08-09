// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::sync::atomic::{AtomicBool, Ordering};
use std::{cell::Ref, sync::Arc};

use crate::blocks::BlockHeader;
use crate::blocks::Tipset;
use crate::chain::{index::ChainIndex, store::ChainStore};
use crate::interpreter::errors::Error;
use crate::networks::ChainConfig;
use crate::shim::{
    gas::{price_list_by_network_version, Gas, GasTracker},
    state_tree::StateTree,
    version::NetworkVersion,
};
use crate::utils::encoding::from_slice_with_fallback;
use anyhow::bail;
use cid::Cid;
use fvm2::externs::{Consensus, Externs, Rand};
use fvm_ipld_blockstore::{
    tracking::{BSStats, TrackingBlockstore},
    Blockstore,
};
use fvm_shared2::{
    address::Address,
    clock::ChainEpoch,
    consensus::{ConsensusFault, ConsensusFaultType},
};
use tracing::error;

use crate::interpreter::resolve_to_key_addr;

pub struct ForestExternsV2<DB> {
    rand: Box<dyn Rand>,
    heaviest_tipset: Arc<Tipset>,
    epoch: ChainEpoch,
    root: Cid,
    chain_index: Arc<ChainIndex<Arc<DB>>>,
    chain_config: Arc<ChainConfig>,
    bail: AtomicBool,
}

impl<DB: Blockstore + Send + Sync + 'static> ForestExternsV2<DB> {
    pub fn new(
        rand: impl Rand + 'static,
        heaviest_tipset: Arc<Tipset>,
        epoch: ChainEpoch,
        root: Cid,
        chain_index: Arc<ChainIndex<Arc<DB>>>,
        chain_config: Arc<ChainConfig>,
    ) -> Self {
        ForestExternsV2 {
            rand: Box::new(rand),
            heaviest_tipset,
            epoch,
            root,
            chain_index,
            chain_config,
            bail: AtomicBool::new(false),
        }
    }

    fn get_lookback_tipset_state_root_for_round(&self, height: ChainEpoch) -> anyhow::Result<Cid> {
        let (_, st) = ChainStore::get_lookback_tipset_for_round(
            self.chain_index.clone(),
            Arc::clone(&self.chain_config),
            Arc::clone(&self.heaviest_tipset),
            height,
        )?;
        Ok(st)
    }

    fn worker_key_at_lookback(
        &self,
        miner_addr: &Address,
        height: ChainEpoch,
    ) -> anyhow::Result<(Address, i64)> {
        if height < self.epoch - self.chain_config.policy.chain_finality {
            bail!(
                "cannot get worker key (current epoch: {}, height: {})",
                self.epoch,
                height
            );
        }

        let prev_root = self.get_lookback_tipset_state_root_for_round(height)?;
        let lb_state = StateTree::new_from_root(Arc::clone(&self.chain_index.db), &prev_root)?;

        let actor = lb_state
            .get_actor(&miner_addr.into())?
            .ok_or_else(|| anyhow::anyhow!("actor not found {:?}", miner_addr))?;

        let tbs = TrackingBlockstore::new(&self.chain_index.db);

        let ms = fil_actor_interface::miner::State::load(&tbs, actor.code, actor.state)?;

        let worker = ms.info(&tbs)?.worker;

        let state = StateTree::new_from_root(Arc::clone(&self.chain_index.db), &self.root)?;

        let addr = resolve_to_key_addr(&state, &tbs, &worker.into())?;

        let network_version = self.chain_config.network_version(self.epoch);
        let gas_used = cal_gas_used_from_stats(tbs.stats.borrow(), network_version)?;

        Ok((addr.into(), gas_used.round_up() as i64))
    }

    fn verify_block_signature(&self, bh: &BlockHeader) -> anyhow::Result<i64, Error> {
        let (worker_addr, gas_used) =
            self.worker_key_at_lookback(&bh.miner_address().into(), bh.epoch())?;

        bh.check_block_signature(&worker_addr.into())?;

        Ok(gas_used)
    }

    pub fn bail(&self) -> bool {
        self.bail.load(Ordering::Relaxed)
    }
}

impl<DB: Blockstore + Send + Sync + 'static> Externs for ForestExternsV2<DB> {}

impl<DB> Rand for ForestExternsV2<DB> {
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

impl<DB: Blockstore + Send + Sync + 'static> Consensus for ForestExternsV2<DB> {
    // See https://github.com/filecoin-project/lotus/blob/v1.18.0/chain/vm/fvm.go#L102-L216 for reference implementation
    fn verify_consensus_fault(
        &self,
        h1: &[u8],
        h2: &[u8],
        extra: &[u8],
    ) -> anyhow::Result<(Option<ConsensusFault>, i64)> {
        let mut total_gas: i64 = 0;

        // Note that block syntax is not validated. Any validly signed block will be
        // accepted pursuant to the below conditions. Whether or not it could
        // ever have been accepted in a chain is not checked/does not matter here.
        // for that reason when checking block parent relationships, rather than
        // instantiating a Tipset to do so (which runs a syntactic check), we do
        // it directly on the CIDs.

        // (0) cheap preliminary checks

        // are blocks the same?
        if h1 == h2 {
            bail!(
                "no consensus fault: submitted blocks are the same: {:?}, {:?}",
                h1,
                h2
            );
        };
        let bh_1 = from_slice_with_fallback::<BlockHeader>(h1)?;
        let bh_2 = from_slice_with_fallback::<BlockHeader>(h2)?;

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
        // block a must be earlier or equal to block b, epoch wise (ie at least as early
        // in the chain).
        if bh_2.epoch() < bh_1.epoch() {
            bail!(
                "first block must not be of higher height than second: {:?}, {:?}",
                bh_1.epoch(),
                bh_2.epoch()
            );
        };

        let mut fault_type: Option<ConsensusFaultType> = None;

        // (2) check for the consensus faults themselves

        // (a) double-fork mining fault
        if bh_1.epoch() == bh_2.epoch() {
            fault_type = Some(ConsensusFaultType::DoubleForkMining);
        };

        // (b) time-offset mining fault
        // strictly speaking no need to compare heights based on double fork mining
        // check above, but at same height this would be a different fault.
        if bh_1.parents() == bh_2.parents() && bh_1.epoch() != bh_2.epoch() {
            fault_type = Some(ConsensusFaultType::TimeOffsetMining);
        };

        // (c) parent-grinding fault
        // Here extra is the "witness", a third block that shows the connection between
        // A and B as A's sibling and B's parent.
        // Specifically, since A is of lower height, it must be that B was mined
        // omitting A from its tipset
        if !extra.is_empty() {
            let bh_3 = from_slice_with_fallback::<BlockHeader>(extra)?;
            if bh_1.parents() == bh_3.parents()
                && bh_1.epoch() == bh_3.epoch()
                && bh_2.parents().cids().contains(bh_3.cid())
                && !bh_2.parents().cids().contains(bh_1.cid())
            {
                fault_type = Some(ConsensusFaultType::ParentGrinding);
            }
        };

        match fault_type {
            None => {
                // (3) return if no consensus fault
                Ok((None, total_gas))
            }
            Some(fault_type) => {
                // (4) expensive final checks

                let bail = |err| {
                    // When a lookup error occurs we should just bail terminating all the
                    // computations.
                    error!("database lookup error: {err}");
                    self.bail.store(true, Ordering::Relaxed);
                    Err(err)
                };

                // check blocks are properly signed by their respective miner
                // note we do not need to check extra's: it is a parent to block b
                // which itself is signed, so it was willingly included by the miner
                for block_header in [&bh_1, &bh_2] {
                    let res = self.verify_block_signature(block_header);
                    match res {
                        // invalid consensus fault: cannot verify block header signature
                        Err(Error::Signature(_)) => return Ok((None, total_gas)),
                        Err(Error::Lookup(err)) => return bail(err),
                        Ok(gas_used) => total_gas += gas_used,
                    }
                }

                let ret = Some(ConsensusFault {
                    target: bh_1.miner_address().into(),
                    epoch: bh_2.epoch(),
                    fault_type,
                });

                Ok((ret, total_gas))
            }
        }
    }
}

fn cal_gas_used_from_stats(
    stats: Ref<BSStats>,
    network_version: NetworkVersion,
) -> anyhow::Result<Gas> {
    let price_list = price_list_by_network_version(network_version);
    let gas_tracker = GasTracker::new(Gas::new(i64::MAX as u64).into(), Gas::new(0).into(), false);
    // num of reads
    for _ in 0..stats.r {
        gas_tracker
            .apply_charge(price_list.on_block_open_base().into())?
            .stop();
    }
    // num of writes
    if stats.w > 0 {
        // total bytes written
        gas_tracker
            .apply_charge(price_list.on_block_link(stats.bw).into())?
            .stop();
        for _ in 1..stats.w {
            gas_tracker
                .apply_charge(price_list.on_block_link(0).into())?
                .stop();
        }
    }
    Ok(gas_tracker.gas_used().into())
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, iter::repeat};

    use anyhow::ensure;

    use super::*;

    #[test]
    fn test_cal_gas_used_from_stats_1_read() -> anyhow::Result<()> {
        test_cal_gas_used_from_stats_inner(1, &[])
    }

    #[test]
    fn test_cal_gas_used_from_stats_1_write() -> anyhow::Result<()> {
        test_cal_gas_used_from_stats_inner(0, &[100])
    }

    #[test]
    fn test_cal_gas_used_from_stats_multi_read() -> anyhow::Result<()> {
        test_cal_gas_used_from_stats_inner(10, &[])
    }

    #[test]
    fn test_cal_gas_used_from_stats_multi_write() -> anyhow::Result<()> {
        test_cal_gas_used_from_stats_inner(0, &[100, 101, 102, 103, 104, 105, 106, 107, 108, 109])
    }

    #[test]
    fn test_cal_gas_used_from_stats_1_read_1_write() -> anyhow::Result<()> {
        test_cal_gas_used_from_stats_inner(1, &[100])
    }

    #[test]
    fn test_cal_gas_used_from_stats_multi_read_multi_write() -> anyhow::Result<()> {
        test_cal_gas_used_from_stats_inner(10, &[100, 101, 102, 103, 104, 105, 106, 107, 108, 109])
    }

    fn test_cal_gas_used_from_stats_inner(
        read_count: usize,
        write_bytes: &[usize],
    ) -> anyhow::Result<()> {
        let network_version = NetworkVersion::V8;
        let stats = BSStats {
            r: read_count,
            w: write_bytes.len(),
            br: 0, // Not used in current logic
            bw: write_bytes.iter().sum(),
        };
        let result = cal_gas_used_from_stats(RefCell::new(stats).borrow(), network_version)?;

        // Simulates logic in old GasBlockStore
        let price_list = price_list_by_network_version(network_version);
        let tracker = GasTracker::new(Gas::new(u64::MAX).into(), Gas::new(0).into(), false);
        repeat(()).take(read_count).for_each(|_| {
            tracker
                .apply_charge(price_list.on_block_open_base().into())
                .unwrap()
                .stop();
        });
        for &bytes in write_bytes {
            tracker
                .apply_charge(price_list.on_block_link(bytes).into())?
                .stop();
        }
        let expected = tracker.gas_used();

        ensure!(result == expected.into());
        Ok(())
    }
}
