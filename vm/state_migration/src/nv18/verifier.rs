// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use ahash::HashMap;
use anyhow::anyhow;
use cid::Cid;
use fil_actor_system_v9::State as SystemStateV9;
use forest_shim::{address::Address, machine::ManifestV2, state_tree::StateTree};
use forest_utils::db::BlockstoreExt;
use fvm_ipld_blockstore::Blockstore;
use log::{info, warn};

use crate::{ActorMigrationVerifier, Migrator};

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
            .get_actor(&Address::new_id(0))?
            .ok_or_else(|| anyhow!("system actor not found"))?;

        let system_actor_state = store
            .get_obj::<SystemStateV9>(&system_actor.state)?
            .ok_or_else(|| anyhow!("system actor state not found"))?;
        let previous_manifest_data = system_actor_state.builtin_actors;

        let previous_manifest = ManifestV2::load(&store, &previous_manifest_data, 1)?;
        let previous_manifest_actors_count = previous_manifest.builtin_actor_codes().count();
        if previous_manifest_actors_count == migrations.len() {
            info!("Migration spec is complete");
        } else {
            warn!(
                "Incomplete migration spec. Count: {}, expected: {}",
                migrations.len(),
                previous_manifest_actors_count
            );
        }

        Ok(())
    }
}
