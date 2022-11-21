use std::collections::HashMap;

use fil_actors_runtime_v8::{Map, MessageAccumulator};
use fvm_ipld_blockstore::Blockstore;
use fvm_shared::{
    address::{Address, Protocol},
    bigint::bigint_ser::BigIntDe,
};
use num_traits::Signed;

use crate::{DataCap, State};

pub struct StateSummary {
    pub verifiers: HashMap<Address, DataCap>,
    pub clients: HashMap<Address, DataCap>,
}

/// Checks internal invariants of verified registry state.
pub fn check_state_invariants<BS: Blockstore>(
    state: &State,
    store: &BS,
) -> (StateSummary, MessageAccumulator) {
    let acc = MessageAccumulator::default();

    // check verifiers
    let mut all_verifiers = HashMap::new();
    match Map::<_, BigIntDe>::load(&state.verifiers, store) {
        Ok(verifiers) => {
            let ret = verifiers.for_each(|key, cap| {
                let verifier = Address::from_bytes(key)?;
                let cap = &cap.0;

                acc.require(
                    verifier.protocol() == Protocol::ID,
                    format!("verifier {verifier} should have ID protocol"),
                );
                acc.require(
                    !cap.is_negative(),
                    format!("verifier {verifier} cap {cap} is negative"),
                );
                all_verifiers.insert(verifier, cap.clone());
                Ok(())
            });

            acc.require_no_error(ret, "error iterating verifiers");
        }
        Err(e) => acc.add(format!("error loading verifiers {e}")),
    }

    // check clients
    let mut all_clients = HashMap::new();
    match Map::<_, BigIntDe>::load(&state.verified_clients, store) {
        Ok(clients) => {
            let ret = clients.for_each(|key, cap| {
                let client = Address::from_bytes(key)?;
                let cap = &cap.0;

                acc.require(
                    client.protocol() == Protocol::ID,
                    format!("client {client} should have ID protocol"),
                );
                acc.require(
                    !cap.is_negative(),
                    format!("client {client} cap {cap} is negative"),
                );
                all_clients.insert(client, cap.clone());
                Ok(())
            });

            acc.require_no_error(ret, "error iterating clients");
        }
        Err(e) => acc.add(format!("error loading clients {e}")),
    }

    // check verifiers and clients are disjoint
    // No need to iterate all clients; any overlap must have been one of all verifiers.
    all_verifiers
        .keys()
        .filter(|verifier| all_clients.contains_key(verifier))
        .for_each(|verifier| {
            acc.add(format!("verifier {verifier} is also a client"));
        });

    (
        StateSummary {
            verifiers: all_verifiers,
            clients: all_clients,
        },
        acc,
    )
}
