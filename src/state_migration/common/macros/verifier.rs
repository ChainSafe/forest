// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

/// Implements `Verifier`, requiring proper `Manifest` types and system actor
/// `State` types being defined by `define_manifests` and `define_system_states`
/// macros respectively.
#[macro_export]
macro_rules! impl_verifier {
    () => {
        pub(super) mod verifier {
            use ahash::HashMap;
            use cid::Cid;
            use forest_shim::{address::Address, state_tree::StateTree};
            use fvm_ipld_blockstore::Blockstore;
            use fvm_ipld_encoding::CborStore;
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
                        .get_cbor::<SystemStateOld>(&system_actor.state)?
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
