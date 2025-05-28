// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use crate::lotus_json::HasLotusJson;
use crate::networks::ACTOR_BUNDLES_METADATA;
use crate::shim::actors::{
    AccountActorStateLoad, CronActorStateLoad, EVMActorStateLoad, InitActorStateLoad,
    MarketActorStateLoad, MinerActorStateLoad, MultisigActorStateLoad, PowerActorStateLoad,
    RewardActorStateLoad, SystemActorStateLoad, account, cron, evm, init, market, miner, multisig,
    power, reward, system,
};
use crate::shim::machine::BuiltinActor;
use ahash::{HashMap, HashMapExt};
use anyhow::{Context, anyhow};
use cid::Cid;
use fvm_ipld_blockstore::Blockstore;
use once_cell::sync::Lazy;
use serde_json::Value;

#[derive(Debug)]
pub struct ActorRegistry {
    map: HashMap<Cid, (BuiltinActor, u64)>,
}

impl ActorRegistry {
    fn new() -> Self {
        let mut map = HashMap::new();
        for ((_, _), metadata) in ACTOR_BUNDLES_METADATA.iter() {
            if let Ok(version) = metadata.actor_major_version() {
                for (actor_type, cid) in metadata.manifest.builtin_actors() {
                    map.insert(cid, (actor_type, version));
                }
            }
        }
        Self { map }
    }

    pub fn get_actor_details_from_code(code_cid: &Cid) -> anyhow::Result<(BuiltinActor, u64)> {
        ACTOR_REGISTRY
            .map
            .get(code_cid)
            .copied()
            .ok_or_else(|| anyhow!("Unknown actor code CID: {}", code_cid))
    }
}

static ACTOR_REGISTRY: Lazy<ActorRegistry> = Lazy::new(ActorRegistry::new);

macro_rules! load_and_serialize_state {
    ($store:expr, $code_cid:expr, $state_cid:expr, $actor_type:expr, $state_type:ty) => {{
        let state = <$state_type>::load($store, *$code_cid, *$state_cid).context(format!(
            "Failed to load {:?} actor state",
            $actor_type.name()
        ))?;

        serde_json::to_value(state.into_lotus_json()).context(format!(
            "Failed to serialize {:?} state to JSON",
            $actor_type.name()
        ))
    }};
}

pub fn load_and_serialize_actor_state<BS>(
    store: &BS,
    code_cid: &Cid,
    state_cid: &Cid,
) -> anyhow::Result<Value>
where
    BS: Blockstore,
{
    let (actor_type, _) = ActorRegistry::get_actor_details_from_code(code_cid)?;
    match actor_type {
        BuiltinActor::Account => {
            load_and_serialize_state!(store, code_cid, state_cid, actor_type, account::State)
        }
        BuiltinActor::Cron => {
            load_and_serialize_state!(store, code_cid, state_cid, actor_type, cron::State)
        }
        BuiltinActor::Miner => {
            load_and_serialize_state!(store, code_cid, state_cid, actor_type, miner::State)
        }
        BuiltinActor::Market => {
            load_and_serialize_state!(store, code_cid, state_cid, actor_type, market::State)
        }
        BuiltinActor::EVM => {
            load_and_serialize_state!(store, code_cid, state_cid, actor_type, evm::State)
        }
        BuiltinActor::System => {
            load_and_serialize_state!(store, code_cid, state_cid, actor_type, system::State)
        }
        BuiltinActor::Init => {
            load_and_serialize_state!(store, code_cid, state_cid, actor_type, init::State)
        }
        BuiltinActor::Power => {
            load_and_serialize_state!(store, code_cid, state_cid, actor_type, power::State)
        }
        BuiltinActor::Multisig => {
            load_and_serialize_state!(store, code_cid, state_cid, actor_type, multisig::State)
        }
        BuiltinActor::Reward => {
            load_and_serialize_state!(store, code_cid, state_cid, actor_type, reward::State)
        }
        // Add other actor types as needed
        _ => Err(anyhow!(
            "No serializer implemented for actor type: {:?}",
            actor_type
        )),
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::db::MemoryDB;
    use crate::utils::multihash::MultihashCode;
    use fvm_ipld_encoding::{DAG_CBOR, to_vec};
    use multihash_derive::MultihashDigest;
    use std::sync::Arc;

    fn get_real_actor_cid(target_actor: BuiltinActor) -> Option<Cid> {
        ACTOR_BUNDLES_METADATA
            .values()
            .flat_map(|metadata| metadata.manifest.builtin_actors())
            .find(|(actor_type, _)| *actor_type == target_actor)
            .map(|(_, cid)| cid)
    }

    #[test]
    fn test_get_actor_details_from_code_success() {
        let account_cid = get_real_actor_cid(BuiltinActor::Account)
            .expect("Should have Account actor in metadata");

        let result = ActorRegistry::get_actor_details_from_code(&account_cid);
        assert!(result.is_ok());

        let (builtin_actor_type, _) = result.unwrap();
        assert_eq!(builtin_actor_type, BuiltinActor::Account);
    }

    #[test]
    fn test_get_actor_details_from_code_multiple_actors() {
        let test_cases = vec![
            (BuiltinActor::Account, "Account"),
            (BuiltinActor::System, "System"),
            (BuiltinActor::Cron, "Cron"),
            (BuiltinActor::Miner, "Miner"),
        ];

        for (expected_actor, actor_name) in test_cases {
            if let Some(cid) = get_real_actor_cid(expected_actor) {
                let result = ActorRegistry::get_actor_details_from_code(&cid);
                assert!(
                    result.is_ok(),
                    "Failed to get details for {} actor",
                    actor_name
                );

                let (builtin_actor_type, _) = result.unwrap();
                assert_eq!(
                    builtin_actor_type, expected_actor,
                    "Wrong actor type returned for {} actor",
                    actor_name
                );
            }
        }
    }

    #[test]
    fn test_details_get_actor_details_from_code_unknown_cid() {
        let unknown_cid = Cid::new_v1(
            DAG_CBOR,
            MultihashCode::Blake2b256.digest(b"unknown_actor_code"),
        );

        let result = ActorRegistry::get_actor_details_from_code(&unknown_cid);
        assert!(result.is_err());

        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("Unknown actor code CID"));
        assert!(error_msg.contains(&unknown_cid.to_string()));
    }

    #[test]
    fn test_basic_load_and_serialize_actor_state_all_supported_actors() {
        let db = Arc::new(MemoryDB::default());

        // Test all supported actor types with real CIDs
        let supported_actors = vec![
            (BuiltinActor::Account, "Account"),
            (BuiltinActor::Cron, "Cron"),
            (BuiltinActor::Miner, "Miner"),
            (BuiltinActor::Market, "Market"),
            (BuiltinActor::EVM, "EVM"),
            (BuiltinActor::System, "System"),
        ];

        for (actor_type, actor_name) in supported_actors {
            if let Some(code_cid) = get_real_actor_cid(actor_type) {
                // Create a minimal valid state for each actor type
                let state_data = match actor_type {
                    BuiltinActor::Account => {
                        let state = account::State::V16(fil_actor_account_state::v16::State {
                            address: crate::shim::address::Address::new_id(1).into(),
                        });
                        to_vec(&state).unwrap()
                    }
                    BuiltinActor::System => {
                        let state = system::State::V16(fil_actor_system_state::v16::State {
                            builtin_actors: Cid::new_v1(
                                DAG_CBOR,
                                MultihashCode::Blake2b256.digest(b"test"),
                            ),
                        });
                        to_vec(&state).unwrap()
                    }
                    BuiltinActor::Cron => {
                        let state = cron::State::V16(fil_actor_cron_state::v16::State {
                            entries: Vec::new(),
                        });
                        to_vec(&state).unwrap()
                    }
                    // For other complicated actors, create minimal state data
                    _ => format!("minimal_{}_state", actor_name.to_lowercase()).into_bytes(),
                };

                let state_cid =
                    Cid::new_v1(DAG_CBOR, MultihashCode::Blake2b256.digest(&state_data));
                db.put_keyed(&state_cid, &state_data).unwrap();

                let result = load_and_serialize_actor_state(db.as_ref(), &code_cid, &state_cid);

                // Some actors might fail due to state format issues, but the function
                // should at least recognize the actor type and attempt to load it
                if result.is_err() {
                    let error_msg = result.unwrap_err().to_string();
                    // Should not be "unknown actor" or "no serializer" errors
                    assert!(
                        !error_msg.contains("Unknown actor code CID")
                            && !error_msg.contains("No serializer implemented"),
                        "Unexpected error for {} actor: {}",
                        actor_name,
                        error_msg
                    );
                }
            }
        }
    }
}
