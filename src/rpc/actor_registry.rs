// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use crate::lotus_json::HasLotusJson;
use crate::networks::ACTOR_BUNDLES_METADATA;
use crate::shim::actors::{
    AccountActorStateLoad, CronActorStateLoad, MinerActorStateLoad, account, cron, miner,
};
use crate::shim::machine::BuiltinActor;
use ahash::{HashMap, HashMapExt};
use anyhow::anyhow;
use cid::Cid;
use fvm_ipld_blockstore::Blockstore;
use once_cell::sync::Lazy;
use serde_json::Value;

// Build a map from CIDs to actor types and versions once at startup
static CID_TO_ACTOR_TYPE: Lazy<HashMap<Cid, (BuiltinActor, u64)>> = Lazy::new(|| {
    let mut map = HashMap::new();

    // Populate the map using data from ACTOR_BUNDLES_METADATA
    for ((_, _), metadata) in ACTOR_BUNDLES_METADATA.iter() {
        if let Ok(version) = metadata.actor_major_version() {
            for (actor_type, cid) in metadata.manifest.builtin_actors() {
                map.insert(cid, (actor_type, version));
            }
        }
    }

    map
});

pub fn get_actor_type_from_code(code_cid: &Cid) -> anyhow::Result<(BuiltinActor, u64)> {
    CID_TO_ACTOR_TYPE
        .get(code_cid)
        .copied()
        .ok_or_else(|| anyhow!("Unknown actor code CID: {}", code_cid))
}

pub fn load_and_serialize_actor_state<BS>(
    store: &BS,
    code_cid: &Cid,
    state_cid: &Cid,
) -> anyhow::Result<Value>
where
    BS: Blockstore,
{
    let (actor_type, _) = get_actor_type_from_code(code_cid)?;

    match actor_type {
        BuiltinActor::Account => {
            let state = account::State::load(store, *code_cid, *state_cid)
                .map_err(|e| anyhow!("Failed to load account actor state: {}", e))?;
            Ok(serde_json::to_value(state.into_lotus_json())
                .map_err(|e| anyhow!("Failed to serialize account state to JSON: {}", e))?)
        }
        BuiltinActor::Cron => {
            let state = cron::State::load(store, *code_cid, *state_cid)
                .map_err(|e| anyhow!("Failed to load cron actor state: {}", e))?;
            Ok(serde_json::to_value(state.into_lotus_json())
                .map_err(|e| anyhow!("Failed to serialize cron state to JSON: {}", e))?)
        }
        BuiltinActor::Miner => {
            let state = miner::State::load(store, *code_cid, *state_cid)
                .map_err(|e| anyhow!("Failed to load miner actor state: {}", e))?;
            Ok(serde_json::to_value(state.into_lotus_json())
                .map_err(|e| anyhow!("Failed to serialize miner state to JSON: {}", e))?)
        }
        // Add other actor types as needed
        _ => Err(anyhow!(
            "No serializer implemented for actor type: {:?}",
            actor_type
        )),
    }
}
