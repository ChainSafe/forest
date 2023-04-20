// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use ahash::HashMap;
use anyhow::anyhow;
use cid::Cid;
use fil_actor_system_v10::State as SystemStateV10;
use forest_shim::{address::Address, machine::Manifest, state_tree::StateTree};
use forest_utils::db::BlockstoreExt;
use fvm_ipld_blockstore::Blockstore;
use log::{debug, warn};

use crate::common::{verifier::ActorMigrationVerifier, Migrator};

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
            .ok_or_else(|| anyhow!("system actor not found"))?;

        let system_actor_state = store
            .get_obj::<SystemStateV10>(&system_actor.state)?
            .ok_or_else(|| anyhow!("system actor state not found"))?;
        let manifest_data = system_actor_state.builtin_actors;

        let manifest = Manifest::load(&store, &manifest_data, 1)?;
        let manifest_actors_count = manifest.builtin_actor_codes().count();
        if manifest_actors_count == migrations.len() {
            debug!("Migration spec is correct.");
        } else {
            warn!(
                "Incomplete migration spec. Count: {}, expected: {}",
                migrations.len(),
                manifest_actors_count
            );
        }

        Ok(())
    }
}
