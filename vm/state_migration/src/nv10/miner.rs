// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::nv10::util::{migrate_amt_raw, migrate_hamt_raw};
use crate::MigrationError;
use crate::MigrationOutput;
use crate::MigrationResult;
use crate::{ActorMigration, ActorMigrationInput};
use actor::miner::{
    SectorOnChainInfo, DEADLINE_EXPIRATIONS_AMT_BITWIDTH, DEADLINE_PARTITIONS_AMT_BITWIDTH,
    PARTITION_EARLY_TERMINATION_ARRAY_AMT_BITWIDTH, PARTITION_EXPIRATION_AMT_BITWIDTH,
    PRECOMMIT_EXPIRY_AMT_BITWIDTH, SECTORS_AMT_BITWIDTH,
}; // FIXME: shouldn't these come from v2? Also most of them require cast from usize -> i32. check
// Right now using them from the current actor crate.
use actor_interface::actorv2::miner::{Deadline as V2Deadline, Deadlines, MinerInfo};
use actor_interface::actorv2::miner::{
    State as V2State, WPOST_PERIOD_DEADLINES as V2_WPOST_PERIOD_DEADLINES,
};
use actor_interface::actorv3::miner::{
    Deadline as V3Deadline, Deadlines as V3Deadlines, PowerPair as V3PowerPair,
    WPOST_PERIOD_DEADLINES as V3_WPOST_PERIOD_DEADLINES,
};
use actor_interface::actorv3::miner::{
    Partition as V3Partition, State as V3MinerState, WorkerKeyChange,
};
use async_std::sync::Arc;
use cid::Cid;
use cid::Code;
use ipld_blockstore::BlockStore;

use actor_interface::actorv3;
use fil_types::ActorID;
use fil_types::HAMT_BIT_WIDTH;
use actor_interface::ActorVersion;
use actor_interface::Array;
use forest_bitfield::BitField;
use ipld_amt::Amt;
use actor::miner::ExpirationSet;

pub struct MinerMigrator;

impl<BS: BlockStore + Send + Sync> ActorMigration<BS> for MinerMigrator {
    fn migrate_state(
        &self,
        store: Arc<BS>,
        input: ActorMigrationInput,
    ) -> MigrationResult<MigrationOutput> {
        let v2_in_state: Option<V2State> = store
            .get(&input.head)
            .map_err(|e| MigrationError::BlockStoreRead(e.to_string()))?;

        let v2_in_state = v2_in_state.unwrap();

        let info_out = self
            .migrate_info(&*store, v2_in_state.info)
            .map_err(|_| MigrationError::Other)?;

        let pre_committed_sectors_out = migrate_hamt_raw::<_, BitField>(
            &*store,
            v2_in_state.pre_committed_sectors,
            HAMT_BIT_WIDTH,
        )
        .map_err(|_| MigrationError::Other)?;

        let pre_committed_sectors_expiry_out = migrate_amt_raw::<_, BitField>(
            &*store,
            v2_in_state.pre_committed_sectors_expiry,
            PRECOMMIT_EXPIRY_AMT_BITWIDTH as i32,
        )
        .map_err(|_| MigrationError::Other)?;

        // TODO load from cache when migration cache is implemented.
        let sectors_out = migrate_amt_raw::<_, SectorOnChainInfo>(
            &*store,
            v2_in_state.sectors,
            SECTORS_AMT_BITWIDTH as i32,
        )
        .map_err(|_| MigrationError::Other)?;

        let deadlines_out = self
            .migrate_deadlines(&*store, v2_in_state.deadlines)
            .map_err(|_| MigrationError::Other)?;

        let out_state = V3MinerState {
            info: info_out,
            pre_commit_deposits: v2_in_state.pre_commit_deposits,
            locked_funds: v2_in_state.locked_funds,
            vesting_funds: v2_in_state.vesting_funds,
            fee_debt: v2_in_state.fee_debt,
            initial_pledge: v2_in_state.initial_pledge,
            pre_committed_sectors: pre_committed_sectors_out,
            pre_committed_sectors_expiry: pre_committed_sectors_expiry_out,
            allocated_sectors: v2_in_state.allocated_sectors,
            sectors: sectors_out,
            proving_period_start: v2_in_state.proving_period_start,
            current_deadline: v2_in_state.current_deadline as usize,
            deadlines: deadlines_out,
            early_terminations: v2_in_state.early_terminations,
        };

        let new_head = store
            .put(&out_state, Code::Blake2b256)
            .map_err(|_| MigrationError::Other)?;

        Ok(MigrationOutput {
            new_code_cid: *actorv3::MINER_ACTOR_CODE_ID,
            new_head,
        })
    }
}

impl MinerMigrator {
    fn migrate_info<BS: BlockStore>(&self, store: &BS, info: Cid) -> MigrationResult<Cid> {
        let old_info: Option<MinerInfo> = store
            .get(&info)
            .map_err(|e| MigrationError::BlockStoreRead(e.to_string()))?;

        let old_info = old_info.unwrap();

        let new_workerkey_change = if let Some(pending_worker_key) = old_info.pending_worker_key {
            Some(WorkerKeyChange {
                new_worker: pending_worker_key.new_worker,
                effective_at: pending_worker_key.effective_at,
            })
        } else {
            None
        };

        let new_workerkey_change = new_workerkey_change.unwrap();

        let window_post_proof = old_info
            .seal_proof_type
            .registered_winning_post_proof()
            .map_err(|_| MigrationError::Other)?; // FIXME should be: registered window post proof.

        let new_info = actorv3::miner::MinerInfo {
            owner: old_info.owner,
            worker: old_info.worker,
            control_addresses: old_info.control_addresses,
            pending_worker_key: Some(new_workerkey_change),
            peer_id: old_info.peer_id,
            multi_address: old_info.multi_address,
            window_post_proof_type: window_post_proof,
            sector_size: old_info.sector_size,
            window_post_partition_sectors: old_info.window_post_partition_sectors,
            consensus_fault_elapsed: old_info.consensus_fault_elapsed,
            pending_owner_address: old_info.pending_owner_address,
        };

        let root = store
            .put(&new_info, Code::Blake2b256)
            .map_err(|_| MigrationError::Other);

        root
    }

    // TODO: might need migration cache here.
    fn migrate_deadlines<BS: BlockStore>(
        &self,
        store: &BS,
        deadlines: Cid,
    ) -> MigrationResult<Cid> {
        let v2_in_deadlines: Option<Deadlines> = store
            .get(&deadlines)
            .map_err(|e| MigrationError::BlockStoreRead(e.to_string()))?;

        let v2_in_deadlines = v2_in_deadlines.unwrap();

        if V2_WPOST_PERIOD_DEADLINES != V3_WPOST_PERIOD_DEADLINES {
            return Err(MigrationError::Other); // FIXME: use descriptive error.
        }

        let mut out_deadlines = V3Deadlines { due: vec![] };

        
        for (i, d) in v2_in_deadlines.due.iter().enumerate() {
            let out_deadline_cid = {
                let in_deadline: Option<V2Deadline> =
                store.get(&d).map_err(|_| MigrationError::Other)?; // FIXME error handline
                let in_deadline = in_deadline.unwrap();
                let partitions = self.migrate_partitions(store, in_deadline.partitions)?;
                let expiration_epochs = migrate_amt_raw::<_, BitField>(
                    store,
                    in_deadline.expirations_epochs,
                    DEADLINE_EXPIRATIONS_AMT_BITWIDTH as i32,
                )
                .map_err(|_| MigrationError::Other)?;
                
                let mut out_deadline = V3Deadline::new(store).map_err(|_| MigrationError::Other)?;
                out_deadline.partitions = partitions;
                out_deadline.expirations_epochs = expiration_epochs;
                out_deadline.partitions_posted = in_deadline.post_submissions;
                out_deadline.early_terminations = in_deadline.early_terminations;
                out_deadline.live_sectors = in_deadline.live_sectors;
                out_deadline.total_sectors = in_deadline.total_sectors;
                out_deadline.faulty_power = V3PowerPair {
                    raw: in_deadline.faulty_power.raw,
                    qa: in_deadline.faulty_power.qa,
                };

                // If there are no live sectors in this partition, zero out the "partitions
                // posted" bitfield. This corrects a state issue where:
                // 1. A proof is submitted and a partition is marked as proven.
                // 2. All sectors in a deadline are terminated during the challenge window.
                // 3. The end of deadline logic is skipped because there are no live sectors.
                // This bug has been fixed in actors v3 (no terminations allowed during the
                // challenge window) but the state still needs to be fixed.
                // See: https://github.com/filecoin-project/specs-actors/issues/1348
                if out_deadline.live_sectors == 0 {
                    out_deadline.partitions_posted = BitField::new()
                }

                store.put(&out_deadline, Code::Blake2b256)
            };

            out_deadlines.due[i] = out_deadline_cid.map_err(|_| MigrationError::Other)?;
        }

        store.put(&out_deadlines, Code::Blake2b256).map_err(|_| MigrationError::Other)
    }

    fn migrate_partitions<BS: BlockStore>(&self, store: &BS, root: Cid) -> MigrationResult<Cid> {
        // AMT<PartitionNumber, Partition>
        let mut in_array =
            Array::load(&root, store, ActorVersion::V2).map_err(|_| MigrationError::Other)?;

        let mut out_array = Amt::new_with_bit_width(store, DEADLINE_PARTITIONS_AMT_BITWIDTH);

        // let v2_in_partition;
        in_array.for_each(|k: u64, part: &V3Partition| {
            let expirations_epochs = migrate_amt_raw::<_, ExpirationSet>(
                store,
                part.expirations_epochs,
                PARTITION_EXPIRATION_AMT_BITWIDTH as i32,
            )
            .map_err(|_| MigrationError::Other)?;

            let early_terminated = migrate_amt_raw::<_, BitField>(
                store,
                part.early_terminated,
                PARTITION_EARLY_TERMINATION_ARRAY_AMT_BITWIDTH as i32,
            )
            .map_err(|_| MigrationError::Other)?;

            let out_partition = V3Partition {
                sectors: part.sectors.clone(),
                unproven: part.unproven.clone(),
                faults: part.faults.clone(),
                recoveries: part.recoveries.clone(),
                terminated: part.terminated.clone(),
                expirations_epochs,
                early_terminated,
                live_power: part.live_power.clone(),
                unproven_power: part.unproven_power.clone(),
                faulty_power: part.faulty_power.clone(),
                recovering_power: part.recovering_power.clone(),
            };

            out_array.set(k as usize, out_partition)?;

            Ok(())
        }).map_err(|_| MigrationError::Other)?;

        in_array.flush().map_err(|_| MigrationError::Other)
    }
}
