// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::state_migration::common::{TypeMigration, TypeMigrator};
use fil_actor_miner_state::{
    v15::VestingFunds as VestingFundsV15,
    v16::{VestingFund as VestingFundV16, VestingFunds as VestingFundsV16},
};
use fvm_ipld_blockstore::Blockstore;

impl TypeMigration<VestingFundsV15, VestingFundsV16> for TypeMigrator {
    fn migrate_type(
        from: VestingFundsV15,
        store: &impl Blockstore,
    ) -> anyhow::Result<VestingFundsV16> {
        let mut to = VestingFundsV16::new();
        let funds = from.funds.into_iter().map(|f| VestingFundV16 {
            epoch: f.epoch,
            amount: f.amount,
        });
        to.save(store, funds)?;
        Ok(to)
    }
}
