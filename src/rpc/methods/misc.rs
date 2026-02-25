// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::collections::BTreeMap;

use cid::Cid;
use enumflags2::BitFlags;
use fvm_ipld_blockstore::Blockstore;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::rpc::eth::CollectedEvent;
use crate::rpc::eth::filter::{ParsedFilter, SkipEvent};
use crate::{
    blocks::TipsetKey,
    lotus_json::{LotusJson, lotus_json_with_self},
    rpc::{ApiPaths, Ctx, Permission, RpcMethod, ServerError, types::EventEntry},
    shim::{address::Address, clock::ChainEpoch},
};

pub enum GetActorEventsRaw {}
impl RpcMethod<1> for GetActorEventsRaw {
    const NAME: &'static str = "Filecoin.GetActorEventsRaw";
    const PARAM_NAMES: [&'static str; 1] = ["eventFilter"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> = Some(
        "Returns all user-programmed and built-in actor events that match the given filter. Results may be limited by MaxFilterResults, MaxFilterHeightRange, and the node's available historical data.",
    );

    type Params = (Option<ActorEventFilter>,);
    type Ok = Vec<ActorEvent>;
    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (filter,): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        if let Some(filter) = filter {
            let parsed_filter = ParsedFilter::from_actor_event_filter(
                ctx.chain_store().heaviest_tipset().epoch(),
                ctx.eth_event_handler.max_filter_height_range,
                filter,
            )?;
            let events = ctx
                .eth_event_handler
                .get_events_for_parsed_filter(&ctx, &parsed_filter, SkipEvent::Never)
                .await?;
            Ok(events.into_iter().map(|ce| ce.into()).collect())
        } else {
            Ok(vec![])
        }
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

#[derive(Debug, PartialEq, Clone, JsonSchema, Serialize, Deserialize)]
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

impl From<CollectedEvent> for ActorEvent {
    fn from(event: CollectedEvent) -> Self {
        ActorEvent {
            entries: event.entries,
            emitter: LotusJson(event.emitter_addr),
            reverted: event.reverted,
            height: event.height,
            tipset_key: LotusJson(event.tipset_key),
            msg_cid: LotusJson(event.msg_cid),
        }
    }
}
