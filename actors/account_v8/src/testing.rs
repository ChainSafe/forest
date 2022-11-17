use fil_actors_runtime_v8::{MessageAccumulator, FIRST_NON_SINGLETON_ADDR};
use fvm_shared::address::{Address, Protocol};

use crate::State;

pub struct StateSummary {
    pub pub_key_address: Address,
}

/// Checks internal invariants of account state.
pub fn check_state_invariants(
    state: &State,
    id_address: &Address,
) -> (StateSummary, MessageAccumulator) {
    let acc = MessageAccumulator::default();

    match id_address.id() {
        Ok(id) if id >= FIRST_NON_SINGLETON_ADDR => {
            acc.require(
                state.address.protocol() == Protocol::BLS
                    || state.address.protocol() == Protocol::Secp256k1,
                format!(
                    "actor address {} must be BLS or SECP256K1 protocol",
                    state.address
                ),
            );
        }
        Err(e) => acc.add(format!("error extracting actor ID from address: {e}")),
        _ => (),
    }

    (
        StateSummary {
            pub_key_address: state.address,
        },
        acc,
    )
}
