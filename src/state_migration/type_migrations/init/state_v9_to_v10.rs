// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use fil_actor_init_state::{v10::State as InitStateV10, v9::State as InitStateV9};
use fil_actors_shared::v10::{make_map_with_root, Map};
use forest_shim::{
    address::{Address, PAYLOAD_HASH_LEN},
    state_tree::ActorID,
};
use fvm_ipld_blockstore::Blockstore;

use crate::common::{TypeMigration, TypeMigrator};

impl TypeMigration<InitStateV9, InitStateV10> for TypeMigrator {
    fn migrate_type(from: InitStateV9, store: &impl Blockstore) -> anyhow::Result<InitStateV10> {
        let mut in_addr_map: Map<_, ActorID> =
            make_map_with_root(&from.address_map, &store).map_err(|e| anyhow::anyhow!("{e}"))?;

        let actor_id = from.next_id;
        let eth_zero_addr = Address::new_delegated(
            Address::ETHEREUM_ACCOUNT_MANAGER_ACTOR.id()?,
            &[0; PAYLOAD_HASH_LEN],
        )?;
        in_addr_map.set(eth_zero_addr.to_bytes().into(), actor_id)?;

        let out_state = InitStateV10 {
            address_map: in_addr_map.flush()?,
            next_id: from.next_id + 1,
            network_name: from.network_name,
        };

        Ok(out_state)
    }
}
