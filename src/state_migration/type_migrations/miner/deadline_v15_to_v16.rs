// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::state_migration::common::{TypeMigration, TypeMigrator};
use fil_actor_miner_state::{
    v15::Deadline as DeadlineV15,
    v16::{Deadline as DeadlineV16, PowerPair as PowerPairV16},
};
use fvm_ipld_blockstore::Blockstore;
use num::Zero;

impl TypeMigration<DeadlineV15, DeadlineV16> for TypeMigrator {
    fn migrate_type(from: DeadlineV15, store: &impl Blockstore) -> anyhow::Result<DeadlineV16> {
        let partitions = from.partitions_amt(store)?;
        let mut to = DeadlineV16 {
            partitions: from.partitions,
            expirations_epochs: from.expirations_epochs,
            partitions_posted: from.partitions_posted,
            early_terminations: from.early_terminations,
            live_sectors: from.live_sectors,
            total_sectors: from.total_sectors,
            faulty_power: PowerPairV16::new(from.faulty_power.raw, from.faulty_power.qa),
            optimistic_post_submissions: from.optimistic_post_submissions,
            sectors_snapshot: from.sectors_snapshot,
            partitions_snapshot: from.partitions_snapshot,
            optimistic_post_submissions_snapshot: from.optimistic_post_submissions_snapshot,
            live_power: PowerPairV16::zero(),
            daily_fee: Zero::zero(),
        };
        // Sum up live power in the partitions of this deadline
        partitions.for_each(|_, p| {
            to.live_power.raw += &p.live_power.raw;
            to.live_power.qa += &p.live_power.qa;
            Ok(())
        })?;
        Ok(to)
    }
}
