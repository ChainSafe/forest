use std::collections::HashMap;

use fil_actors_runtime_v8::{Map, MessageAccumulator, FIRST_NON_SINGLETON_ADDR};
use fvm_ipld_blockstore::Blockstore;
use fvm_shared::{
    address::{Address, Protocol},
    ActorID,
};

use crate::State;

pub struct StateSummary {
    pub ids_by_address: HashMap<Address, ActorID>,
    pub next_id: ActorID,
}

// Checks internal invariants of init state.
pub fn check_state_invariants<BS: Blockstore>(
    state: &State,
    store: &BS,
) -> (StateSummary, MessageAccumulator) {
    let acc = MessageAccumulator::default();

    acc.require(!state.network_name.is_empty(), "network name is empty");
    acc.require(
        state.next_id >= FIRST_NON_SINGLETON_ADDR,
        format!("next id {} is too low", state.next_id),
    );

    let mut init_summary = StateSummary {
        ids_by_address: HashMap::new(),
        next_id: state.next_id,
    };

    let mut address_by_id = HashMap::<ActorID, Address>::new();
    match Map::<_, ActorID>::load(&state.address_map, store) {
        Ok(address_map) => {
            let ret = address_map.for_each(|key, actor_id| {
                let key_address = Address::from_bytes(key)?;

                acc.require(
                    key_address.protocol() != Protocol::ID,
                    format!("key {key_address} is an ID address"),
                );
                acc.require(
                    actor_id >= &FIRST_NON_SINGLETON_ADDR,
                    format!("unexpected singleton ID value {actor_id}"),
                );

                if let Some(duplicate) = address_by_id.insert(*actor_id, key_address) {
                    acc.add(format!(
                        "duplicate mapping to ID {actor_id}: {key_address} {duplicate}"
                    ));
                }
                init_summary.ids_by_address.insert(key_address, *actor_id);

                Ok(())
            });

            acc.require_no_error(ret, "error iterating address map");
        }
        Err(e) => acc.add(format!("error loading address map: {e}")),
    }

    (init_summary, acc)
}
