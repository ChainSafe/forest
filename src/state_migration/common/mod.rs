// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Common code that's shared across all migration code.
//! Each network upgrade / state migration code lives in their own module.

use std::num::NonZeroUsize;

use crate::{
    prelude::*,
    shim::{address::Address, clock::ChainEpoch, econ::TokenAmount, state_tree::StateTree},
    utils::cache::SizeTrackingCache,
};

mod macros;
mod migration_job;
pub(in crate::state_migration) mod migrators;
mod state_migration;
pub(in crate::state_migration) mod verifier;

pub(in crate::state_migration) use state_migration::StateMigration;
pub(in crate::state_migration) type Migrator<BS> = Arc<dyn ActorMigration<BS> + Send + Sync>;

/// Cache of existing CID to CID migrations for an actor.
#[derive(derive_more::Deref)]
pub(in crate::state_migration) struct MigrationCache(SizeTrackingCache<String, CidWrapper>);

impl MigrationCache {
    pub fn new(size: NonZeroUsize) -> Self {
        Self(SizeTrackingCache::new_with_metrics("migration", size))
    }

    pub fn get(&self, key: &str) -> Option<Cid> {
        self.deref().get_cloned(key).map(From::from)
    }

    pub fn get_or_insert_with<F>(&self, key: &str, f: F) -> anyhow::Result<Cid>
    where
        F: FnOnce() -> anyhow::Result<Cid>,
    {
        self.deref()
            .get_or_insert_with(key, || {
                let cid = f()?;
                Ok(cid.into())
            })
            .map(From::from)
    }

    pub fn push(&self, key: String, value: Cid) {
        self.deref().push(key, value.into());
    }
}

impl ShallowClone for MigrationCache {
    fn shallow_clone(&self) -> Self {
        Self(self.deref().shallow_clone())
    }
}

#[allow(dead_code)] // future migrations might need the fields.
pub(in crate::state_migration) struct ActorMigrationInput {
    /// Actor's address
    pub address: Address,
    /// Actor's balance
    pub balance: TokenAmount,
    /// Actor's state head CID
    pub head: Cid,
    /// Epoch of last state transition prior to migration
    pub prior_epoch: ChainEpoch,
    /// Cache of existing CID to CID migrations for this actor
    pub cache: MigrationCache,
}

/// Output of actor migration job.
pub(in crate::state_migration) struct ActorMigrationOutput {
    /// New CID for the actor
    pub new_code_cid: Cid,
    /// New state head CID
    pub new_head: Cid,
}

/// Trait that defines the interface for actor migration job.
pub(in crate::state_migration) trait ActorMigration<BS: Blockstore> {
    fn migrate_state(
        &self,
        store: &BS,
        input: ActorMigrationInput,
    ) -> anyhow::Result<Option<ActorMigrationOutput>>;

    /// Some migration jobs might need to be deferred to be executed after the regular state migration.
    /// These may require some metadata collected during other migrations.
    fn is_deferred(&self) -> bool {
        false
    }
}

/// Trait that defines the interface for actor migration job to be executed after the state migration.
pub(in crate::state_migration) trait PostMigrator<BS: Blockstore>:
    Send + Sync
{
    fn post_migrate_state(&self, store: &BS, actors_out: &mut StateTree<BS>) -> anyhow::Result<()>;
}

/// Trait defining the interface for actor migration verifier.
pub(in crate::state_migration) trait PostMigrationCheck<BS: Blockstore>:
    Send + Sync
{
    fn post_migrate_check(&self, store: &BS, actors_out: &StateTree<BS>) -> anyhow::Result<()>;
}

/// Sized wrapper of [`PostMigrator`].
pub(in crate::state_migration) type PostMigratorArc<BS> = Arc<dyn PostMigrator<BS>>;

/// Sized wrapper of [`PostMigrationCheck`].
pub(in crate::state_migration) type PostMigrationCheckArc<BS> = Arc<dyn PostMigrationCheck<BS>>;

/// Trait that migrates from one data structure to another, similar to
/// [`std::convert::TryInto`] trait but taking an extra block store parameter
pub(in crate::state_migration) trait TypeMigration<From, To> {
    fn migrate_type(from: From, store: &impl Blockstore) -> anyhow::Result<To>;
}

/// Type that implements [`TypeMigration`] for different type pairs. Prefer
/// using a single `struct` so that the compiler could catch duplicate
/// implementations
pub(in crate::state_migration) struct TypeMigrator;

#[cfg(test)]
mod tests {
    use super::MigrationCache;
    use crate::utils::cid::CidCborExt;
    use cid::Cid;
    use nonzero_ext::nonzero;

    #[test]
    fn test_migration_cache() {
        let cache = MigrationCache::new(nonzero!(10usize));
        let cid = Cid::from_cbor_blake2b256(&42).unwrap();
        cache.push("Cthulhu".to_owned(), cid);
        assert_eq!(cache.get("Cthulhu"), Some(cid));
        assert_eq!(cache.get("Ao"), None);

        let cid = Cid::from_cbor_blake2b256(&666).unwrap();
        assert_eq!(cache.get("Azathoth"), None);

        let value = cache.get_or_insert_with("Azathoth", || Ok(cid)).unwrap();
        assert_eq!(value, cid);
        assert_eq!(cache.get("Azathoth"), Some(cid));

        // Tests that there is no deadlock when inserting a value while reading the cache.
        let value = cache
            .get_or_insert_with("Dagon", || Ok(cache.get("Azathoth").unwrap()))
            .unwrap();
        assert_eq!(value, cid);
        assert_eq!(cache.get("Dagon"), Some(cid));
    }
}
