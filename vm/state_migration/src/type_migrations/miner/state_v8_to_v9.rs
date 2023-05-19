// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use cid::multihash::Code::Blake2b256;
use fil_actor_miner_v8::{MinerInfo as MinerInfoV8, State as MinerStateV8};
use fil_actor_miner_v9::{MinerInfo as MinerInfoV9, State as MinerStateV9};
use forest_utils::db::BlockstoreExt;
use fvm_ipld_blockstore::Blockstore;

use crate::common::{TypeMigration, TypeMigrator};

impl TypeMigration<MinerStateV8, MinerStateV9> for TypeMigrator {
    fn migrate_type(from: MinerStateV8, store: &impl Blockstore) -> anyhow::Result<MinerStateV9> {
        let in_info: MinerInfoV8 = store
            .get_obj(&from.info)?
            .ok_or_else(|| anyhow::anyhow!("Miner info: could not read v8 state"))?;

        let out_info: MinerInfoV9 = TypeMigrator::migrate_type(in_info, store)?;

        let out_state = MinerStateV9 {
            info: store.put_obj(&out_info, Blake2b256)?,
            pre_commit_deposits: from.pre_commit_deposits,
            locked_funds: from.locked_funds,
            vesting_funds: from.vesting_funds,
            fee_debt: from.fee_debt,
            initial_pledge: from.initial_pledge,
            pre_committed_sectors: from.pre_committed_sectors,
            pre_committed_sectors_cleanup: from.pre_committed_sectors_cleanup,
            allocated_sectors: from.allocated_sectors,
            sectors: from.sectors,
            proving_period_start: from.proving_period_start,
            current_deadline: from.current_deadline,
            deadlines: from.deadlines,
            early_terminations: from.early_terminations,
            deadline_cron_active: from.deadline_cron_active,
        };

        Ok(out_state)
    }
}
