// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::state_migration::common::{TypeMigration, TypeMigrator};
use crate::utils::db::CborStoreExt;
use fil_actor_miner_state::{
    v15::Deadline as DeadlineV15, v15::Deadlines as DeadlinesV15, v16::Deadline as DeadlineV16,
    v16::Deadlines as DeadlinesV16,
};
use fvm_ipld_blockstore::Blockstore;

impl TypeMigration<DeadlinesV15, DeadlinesV16> for TypeMigrator {
    fn migrate_type(from: DeadlinesV15, store: &impl Blockstore) -> anyhow::Result<DeadlinesV16> {
        let mut to = DeadlinesV16 {
            due: Vec::with_capacity(from.due.len()),
        };
        for old_deadline_cid in from.due {
            let old_deadline: DeadlineV15 = store.get_cbor_required(&old_deadline_cid)?;
            let new_deadline: DeadlineV16 = TypeMigrator::migrate_type(old_deadline, store)?;
            let new_deadline_cid = store.put_cbor_default(&new_deadline)?;
            to.due.push(new_deadline_cid);
        }
        Ok(to)
    }
}
