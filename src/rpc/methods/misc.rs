// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{any::Any, collections::BTreeMap};

use crate::{
    blocks::TipsetKey,
    lotus_json::{lotus_json_with_self, LotusJson},
    rpc::{types::EventEntry, ApiPaths, Ctx, Permission, RpcMethod, ServerError},
    shim::{address::Address, clock::ChainEpoch},
};
use cid::Cid;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub enum GetActorEventsRaw {}
impl RpcMethod<1> for GetActorEventsRaw {
    const NAME: &'static str = "Filecoin.GetActorEventsRaw";
    const PARAM_NAMES: [&'static str; 1] = ["eventFilter"];
    const API_PATHS: ApiPaths = ApiPaths::V1;
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> = Some("Returns all user-programmed and built-in actor events that match the given filter. Results may be limited by MaxFilterResults, MaxFilterHeightRange, and the node's available historical data.");

    type Params = (Option<ActorEventFilter>,);
    type Ok = Vec<ActorEvent>;
    async fn handle(_: Ctx<impl Any>, (_,): Self::Params) -> Result<Self::Ok, ServerError> {
        Err(ServerError::stubbed_for_openrpc())
    }
}

#[derive(Clone, JsonSchema, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActorEventFilter {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub addresses: Vec<LotusJson<Address>>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub fields: BTreeMap<String, Vec<ActorEventBlock>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_height: Option<ChainEpoch>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to_height: Option<ChainEpoch>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tipset_key: Option<LotusJson<TipsetKey>>,
}

#[derive(Clone, JsonSchema, Serialize, Deserialize)]
pub struct ActorEventBlock {
    pub codec: u64,
    pub value: LotusJson<Vec<u8>>,
}

#[derive(Clone, JsonSchema, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActorEvent {
    pub entries: Vec<EventEntry>,
    pub emitter: LotusJson<Address>,
    pub reverted: bool,
    pub height: ChainEpoch,
    pub tipset_key: LotusJson<TipsetKey>,
    pub msg_cid: LotusJson<Cid>,
}

lotus_json_with_self! {
    ActorEvent,
    ActorEventFilter
}
