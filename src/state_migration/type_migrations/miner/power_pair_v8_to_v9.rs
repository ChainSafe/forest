// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use fil_actor_miner_state::{v8::PowerPair as PowerPairV8, v9::PowerPair as PowerPairV9};
use fvm_ipld_blockstore::Blockstore;

use super::super::super::common::{TypeMigration, TypeMigrator};

impl TypeMigration<PowerPairV8, PowerPairV9> for TypeMigrator {
    fn migrate_type(from: PowerPairV8, _: &impl Blockstore) -> anyhow::Result<PowerPairV9> {
        let out = PowerPairV9 {
            raw: from.raw,
            qa: from.qa,
        };

        Ok(out)
    }
}
