// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::sync::Arc;

use crate::shim::state_tree::StateTree;
use ahash::HashMap;
use cid::Cid;

use super::Migrator;

/// The implementation should verify that the migration specification is
/// correct. This is to prevent accidental migration errors.
pub(in crate::state_migration) trait ActorMigrationVerifier<BS> {
    fn verify_migration(
        &self,
        store: &BS,
        migrations: &HashMap<Cid, Migrator<BS>>,
        actors_in: &StateTree<BS>,
    ) -> anyhow::Result<()>;
}

/// Type implementing the `ActorMigrationVerifier` trait.
pub(in crate::state_migration) type MigrationVerifier<BS> =
    Arc<dyn ActorMigrationVerifier<BS> + Send + Sync>;
