// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
pub mod eam;
pub mod eth_account;
mod init;
pub mod migration;
mod system;
pub mod verifier;

use anyhow::{anyhow, bail};
use cid::Cid;
use fil_actor_system_v9::State as SystemStateV9;
use forest_shim::{
    address::Address,
    machine::{Manifest, ManifestV2},
    state_tree::StateTree,
};
use forest_utils::db::BlockstoreExt;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::CborStore;

use crate::{nil_migrator, StateMigration};

impl<BS: Blockstore + Clone + Send + Sync> StateMigration<BS> {
    pub fn add_nv18_migrations(
        &mut self,
        store: BS,
        state: &Cid,
        new_manifest: &Cid,
    ) -> anyhow::Result<()> {
        let state_tree = StateTree::new_from_root(store.clone(), state)?;
        let system_actor = state_tree
            .get_actor(&Address::new_id(0))?
            .ok_or_else(|| anyhow!("system actor not found"))?;

        let system_actor_state = store
            .get_obj::<SystemStateV9>(&system_actor.state)?
            .ok_or_else(|| anyhow!("system actor state not found"))?;
        let current_manifest_data = system_actor_state.builtin_actors;
        let current_manifest = ManifestV2::load(&store, &current_manifest_data, 1)?;

        let (version, new_manifest_data): (u32, Cid) = store
            .get_cbor(new_manifest)?
            .ok_or_else(|| anyhow!("new manifest not found"))?;
        let new_manifest = Manifest::load(&store, &new_manifest_data, version)?;

        current_manifest.builtin_actor_codes().for_each(|code| {
            let id = current_manifest.id_by_code(code);
            let new_code = new_manifest.code_by_id(id).unwrap();
            self.add_migrator(*code, nil_migrator(*new_code));
        });

        self.add_migrator(
            *current_manifest.get_init_code(),
            init::init_migrator(*new_manifest.get_init_code()),
        );

        self.add_migrator(
            *current_manifest.get_system_code(),
            system::system_migrator(new_manifest_data, *new_manifest.get_system_code()),
        );

        Ok(())
    }
}
