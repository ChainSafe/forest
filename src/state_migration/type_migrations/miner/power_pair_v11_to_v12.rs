// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use fil_actor_miner_state::{v11::PowerPair as PowerPairV11, v12::PowerPair as PowerPairV12};
use fvm_ipld_blockstore::Blockstore;

use super::super::super::common::{TypeMigration, TypeMigrator};

impl TypeMigration<PowerPairV11, PowerPairV12> for TypeMigrator {
    fn migrate_type(from: PowerPairV11, _: &impl Blockstore) -> anyhow::Result<PowerPairV12> {
        let out = PowerPairV12 {
            raw: from.raw,
            qa: from.qa,
        };

        Ok(out)
    }
}
