// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

/// Implements `Verifier`, requiring proper `Manifest` types and system actor
/// `State` types being defined by `define_manifests` and `define_system_states`
/// macros respectively.
#[macro_export]
macro_rules! impl_verifier {
    () => {
        pub(super) mod verifier {
            use $crate::cid_collections::CidHashMap;
            use fvm_ipld_blockstore::Blockstore;
            use fvm_ipld_encoding::CborStore;
            use $crate::shim::{address::Address, machine::BuiltinActorManifest, state_tree::StateTree};
            use $crate::state_migration::common::{verifier::ActorMigrationVerifier, Migrator};

            use super::*;

            #[derive(Default)]
            pub struct Verifier {}

            impl<BS: Blockstore> ActorMigrationVerifier<BS> for Verifier {
                fn verify_migration(
                    &self,
                    store: &BS,
                    migrations: &CidHashMap<Migrator<BS>>,
                    actors_in: &StateTree<BS>,
                ) -> anyhow::Result<()> {
                    let system_actor = actors_in
                        .get_actor(&Address::SYSTEM_ACTOR)?
                        .ok_or_else(|| anyhow::anyhow!("system actor not found"))?;

                    let system_actor_state = store
                        .get_cbor::<SystemStateOld>(&system_actor.state)?
                        .ok_or_else(|| anyhow::anyhow!("system actor state not found"))?;
                    let manifest =
                        BuiltinActorManifest::load_v1_actor_list(&store, &system_actor_state.builtin_actors)?;
                    let manifest_actors_count = manifest.builtin_actors().len();
                    if manifest_actors_count == migrations.len() {
                        tracing::debug!("Migration spec is correct.");
                    } else {
                        tracing::warn!(
                            "Incomplete migration spec. Count: {}, expected: {manifest_actors_count}",
                            migrations.len()
                        );
                    }

                    Ok(())
                }
            }
        }
    };
}
