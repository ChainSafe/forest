// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! This module contains the migration logic for the `NV17` upgrade for the market
//! actor.

use std::sync::Arc;

use cid::Cid;
use fvm_ipld_blockstore::Blockstore;

use crate::common::{ActorMigration, ActorMigrationInput, ActorMigrationOutput};

pub struct MarketMigrator;

pub(crate) fn market_migrator<BS>(
    out_code: Cid,
    store: &BS,
    market_proposals: Cid,
) -> anyhow::Result<Arc<dyn ActorMigration<BS> + Send + Sync>>
where
    BS: Blockstore + Clone + Send + Sync,
{
    Ok(Arc::new(MarketMigrator))
}

impl<BS> ActorMigration<BS> for MarketMigrator
where
    BS: Blockstore + Clone + Send + Sync,
{
    fn migrate_state(
        &self,
        store: BS,
        input: ActorMigrationInput,
    ) -> anyhow::Result<ActorMigrationOutput> {
        todo!()
    }
}
