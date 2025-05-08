// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use crate::lotus_json::HasLotusJson;
use crate::networks::ACTOR_BUNDLES_METADATA;
use crate::shim::actors::{AccountActorStateLoad, CronActorStateLoad, account, cron};
use crate::shim::machine::BuiltinActor;
use ahash::{HashMap, HashMapExt};
use anyhow::anyhow;
use cid::Cid;
use fvm_ipld_blockstore::Blockstore;
use once_cell::sync::Lazy;
use serde_json::Value;

static CID_TO_ACTOR_TYPE: Lazy<HashMap<Cid, BuiltinActor>> = Lazy::new(|| {
    let mut map = HashMap::new();

    // Populate the map using data from ACTOR_BUNDLES_METADATA
    for ((_, _), metadata) in ACTOR_BUNDLES_METADATA.iter() {
        for (actor_type, cid) in metadata.manifest.builtin_actors() {
            map.insert(cid, actor_type);
        }
    }

    map
});

pub fn get_actor_type_from_code(code_cid: &Cid) -> anyhow::Result<BuiltinActor> {
    CID_TO_ACTOR_TYPE
        .get(code_cid)
        .copied()
        .ok_or_else(|| anyhow!("Unknown actor code CID: {}", code_cid))
}

pub(crate) fn load_and_serialize_actor_state<BS>(
    store: &BS,
    code_cid: &Cid,
    state_cid: &Cid,
) -> anyhow::Result<Value>
where
    BS: Blockstore,
{
    let actor_type = get_actor_type_from_code(code_cid)?;

    match actor_type {
        BuiltinActor::Account => serialize_account_state(store, code_cid, state_cid),
        BuiltinActor::Cron => serialize_cron_state(store, code_cid, state_cid),
        // Other actor types...
        _ => Err(anyhow!(
            "No serializer implemented for actor type: {:?}",
            actor_type
        )),
    }
}

fn serialize_cron_state<BS>(store: &BS, code_cid: &Cid, state_cid: &Cid) -> anyhow::Result<Value>
where
    BS: Blockstore,
{
    let state = cron::State::load(store, *code_cid, *state_cid)?;
    Ok(serde_json::to_value(state.into_lotus_json())?)
}

fn serialize_account_state<BS>(store: &BS, code_cid: &Cid, state_cid: &Cid) -> anyhow::Result<Value>
where
    BS: Blockstore,
{
    let state = account::State::load(store, *code_cid, *state_cid)?;
    Ok(serde_json::to_value(state.into_lotus_json())?)
}
