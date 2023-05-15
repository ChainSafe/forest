// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#[macro_export(local_inner_macros)]
macro_rules! define_system_states {
    ($state_old:ty, $state_new:ty) => {
        type SystemStateOld = $state_old;
        type SystemStateNew = $state_new;
    };
}

#[macro_export(local_inner_macros)]
macro_rules! define_manifests {
    ($manifest_old:ty, $manifest_new:ty) => {
        type ManifestOld = $manifest_old;
        type ManifestNew = $manifest_new;
    };
}

#[macro_export(local_inner_macros)]
macro_rules! impl_system {
    () => {
        pub(super) mod system {
            use std::sync::Arc;

            use cid::{multihash::Code::Blake2b256, Cid};
            use forest_utils::db::BlockstoreExt;
            use fvm_ipld_blockstore::Blockstore;
            use $crate::common::*;

            pub(super) fn system_migrator<BS: Blockstore + Clone + Send + Sync>(
                new_builtin_actors_cid: Cid,
                new_code_cid: Cid,
            ) -> Arc<dyn ActorMigration<BS> + Send + Sync> {
                Arc::new(SystemMigrator {
                    new_builtin_actors_cid,
                    new_code_cid,
                })
            }

            pub struct SystemMigrator {
                new_builtin_actors_cid: Cid,
                new_code_cid: Cid,
            }

            impl<BS: Blockstore + Clone + Send + Sync> ActorMigration<BS> for SystemMigrator {
                fn migrate_state(
                    &self,
                    store: BS,
                    _input: ActorMigrationInput,
                ) -> anyhow::Result<ActorMigrationOutput> {
                    let state = super::SystemStateNew {
                        builtin_actors: self.new_builtin_actors_cid,
                    };
                    let new_head = store.put_obj(&state, Blake2b256)?;

                    Ok(ActorMigrationOutput {
                        new_code_cid: self.new_code_cid,
                        new_head,
                    })
                }
            }
        }
    };
}

#[macro_export(local_inner_macros)]
macro_rules! impl_verifier {
    () => {
        pub(super) mod verifier {
            use ahash::HashMap;
            use cid::Cid;
            use forest_shim::{address::Address, state_tree::StateTree};
            use forest_utils::db::BlockstoreExt;
            use fvm_ipld_blockstore::Blockstore;
            use $crate::common::{verifier::ActorMigrationVerifier, Migrator};

            use super::*;

            #[derive(Default)]
            pub struct Verifier {}

            impl<BS: Blockstore + Clone + Send + Sync> ActorMigrationVerifier<BS> for Verifier {
                fn verify_migration(
                    &self,
                    store: &BS,
                    migrations: &HashMap<Cid, Migrator<BS>>,
                    actors_in: &StateTree<BS>,
                ) -> anyhow::Result<()> {
                    let system_actor = actors_in
                        .get_actor(&Address::SYSTEM_ACTOR)?
                        .ok_or_else(|| anyhow::anyhow!("system actor not found"))?;

                    let system_actor_state = store
                        .get_obj::<SystemStateOld>(&system_actor.state)?
                        .ok_or_else(|| anyhow::anyhow!("system actor state not found"))?;
                    let manifest_data = system_actor_state.builtin_actors;

                    let manifest = ManifestOld::load(&store, &manifest_data, 1)?;
                    let manifest_actors_count = manifest.builtin_actor_codes().count();
                    if manifest_actors_count == migrations.len() {
                        log::debug!("Migration spec is correct.");
                    } else {
                        log::warn!(
                            "Incomplete migration spec. Count: {}, expected: {}",
                            migrations.len(),
                            manifest_actors_count
                        );
                    }

                    Ok(())
                }
            }
        }
    };
}
