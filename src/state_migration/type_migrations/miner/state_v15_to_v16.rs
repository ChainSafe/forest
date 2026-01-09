// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::state_migration::common::{TypeMigration, TypeMigrator};
use crate::utils::db::CborStoreExt as _;
use fil_actor_miner_state::{
    v15::State as MinerStateV15,
    v16::{Deadlines as DeadlinesV16, State as MinerStateV16, VestingFunds as VestingFundsV16},
};
use fvm_ipld_blockstore::Blockstore;

impl TypeMigration<MinerStateV15, MinerStateV16> for TypeMigrator {
    fn migrate_type(from: MinerStateV15, store: &impl Blockstore) -> anyhow::Result<MinerStateV16> {
        let vesting_funds = {
            let old_vesting_funds = from.load_vesting_funds(store)?;
            let new_vesting_funds: VestingFundsV16 =
                TypeMigrator::migrate_type(old_vesting_funds, store)?;
            new_vesting_funds
        };
        let deadlines = {
            let old_deadlines = from.load_deadlines(store)?;
            let new_deadlines: DeadlinesV16 = TypeMigrator::migrate_type(old_deadlines, store)?;
            store.put_cbor_default(&new_deadlines)?
        };
        let to = MinerStateV16 {
            info: from.info,
            pre_commit_deposits: from.pre_commit_deposits,
            locked_funds: from.locked_funds,
            vesting_funds,
            fee_debt: from.fee_debt,
            initial_pledge: from.initial_pledge,
            pre_committed_sectors: from.pre_committed_sectors,
            pre_committed_sectors_cleanup: from.pre_committed_sectors_cleanup,
            allocated_sectors: from.allocated_sectors,
            sectors: from.sectors,
            proving_period_start: from.proving_period_start,
            current_deadline: from.current_deadline,
            deadlines,
            early_terminations: from.early_terminations,
            deadline_cron_active: from.deadline_cron_active,
        };
        Ok(to)
    }
}
