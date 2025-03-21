// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

/// Define type aliases for system actor `State` types before and after the
/// state migration, namely `SystemStateOld` and `SystemStateNew`
#[macro_export]
macro_rules! define_system_states {
    ($state_old:ty, $state_new:ty) => {
        type SystemStateOld = $state_old;
        type SystemStateNew = $state_new;
    };
}

/// Implements `fn system_migrator`, requiring proper system actor `State` types
/// being defined by `define_system_states` macro.
#[macro_export]
macro_rules! impl_system {
    () => {
        pub(super) mod system {
            use std::sync::Arc;

            use cid::Cid;
            use fvm_ipld_blockstore::Blockstore;
            use $crate::shim::machine::BuiltinActorManifest;
            use $crate::state_migration::common::*;
            use $crate::utils::db::CborStoreExt;

            pub(super) fn system_migrator<BS: Blockstore>(
                new_manifest: &BuiltinActorManifest,
            ) -> Arc<dyn ActorMigration<BS> + Send + Sync> {
                Arc::new(SystemMigrator {
                    new_builtin_actors_cid: new_manifest.source_cid(),
                    new_code_cid: new_manifest.get_system(),
                })
            }

            pub struct SystemMigrator {
                new_builtin_actors_cid: Cid,
                new_code_cid: Cid,
            }

            impl<BS: Blockstore> ActorMigration<BS> for SystemMigrator {
                fn migrate_state(
                    &self,
                    store: &BS,
                    _input: ActorMigrationInput,
                ) -> anyhow::Result<Option<ActorMigrationOutput>> {
                    let state = super::SystemStateNew {
                        builtin_actors: self.new_builtin_actors_cid,
                    };
                    let new_head = store.put_cbor_default(&state)?;

                    Ok(Some(ActorMigrationOutput {
                        new_code_cid: self.new_code_cid,
                        new_head,
                    }))
                }
            }
        }
    };
}
