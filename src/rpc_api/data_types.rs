// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::sync::Arc;

use crate::beacon::BeaconSchedule;
use crate::blocks::TipsetKeys;
use crate::chain::ChainStore;
use crate::chain_sync::{BadBlockCache, SyncState};
use crate::ipld::json::IpldJson;
use crate::key_management::KeyStore;
pub use crate::libp2p::{Multiaddr, Protocol};
use crate::libp2p::{Multihash, NetworkMessage};
use crate::lotus_json::lotus_json_with_self;
use crate::lotus_json::{HasLotusJson, LotusJson};
use crate::message::signed_message::SignedMessage;
use crate::message_pool::{MessagePool, MpoolRpcProvider};
use crate::shim::executor::Receipt;
use crate::shim::{econ::TokenAmount, message::Message};
use crate::state_manager::StateManager;
use ahash::HashSet;
use chrono::Utc;
use cid::Cid;
use fil_actor_interface::market::{DealProposal, DealState};
use fil_actor_interface::miner::MinerPower;
use fil_actor_interface::power::Claim;
use fvm_ipld_blockstore::Blockstore;
use jsonrpc_v2::{MapRouter as JsonRpcMapRouter, Server as JsonRpcServer};
use libipld_core::ipld::Ipld;
use parking_lot::RwLock as SyncRwLock;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::RwLock;

/// This is where you store persistent data, or at least access to stateful
/// data.
pub struct RPCState<DB>
where
    DB: Blockstore,
{
    pub keystore: Arc<RwLock<KeyStore>>,
    pub chain_store: Arc<ChainStore<DB>>,
    pub state_manager: Arc<StateManager<DB>>,
    pub mpool: Arc<MessagePool<MpoolRpcProvider<DB>>>,
    pub bad_blocks: Arc<BadBlockCache>,
    pub sync_state: Arc<SyncRwLock<SyncState>>,
    pub network_send: flume::Sender<NetworkMessage>,
    pub network_name: String,
    pub start_time: chrono::DateTime<Utc>,
    pub beacon: Arc<BeaconSchedule>,
    pub gc_event_tx: flume::Sender<flume::Sender<anyhow::Result<()>>>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct RPCSyncState {
    #[serde(with = "crate::lotus_json")]
    pub active_syncs: Vec<SyncState>,
}

lotus_json_with_self!(RPCSyncState);

pub type JsonRpcServerState = Arc<JsonRpcServer<JsonRpcMapRouter>>;

// Chain API
#[derive(Serialize, Deserialize)]
pub struct BlockMessages {
    #[serde(rename = "BlsMessages", with = "crate::lotus_json")]
    pub bls_msg: Vec<Message>,
    #[serde(rename = "SecpkMessages", with = "crate::lotus_json")]
    pub secp_msg: Vec<SignedMessage>,
    #[serde(rename = "Cids", with = "crate::lotus_json")]
    pub cids: Vec<Cid>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct MessageSendSpec {
    #[serde(with = "crate::lotus_json")]
    max_fee: TokenAmount,
}

lotus_json_with_self!(MessageSendSpec);

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct MarketDeal {
    pub proposal: DealProposal,
    pub state: DealState,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct MessageLookup {
    #[serde(with = "crate::lotus_json")]
    pub receipt: Receipt,
    #[serde(rename = "TipSet", with = "crate::lotus_json")]
    pub tipset: TipsetKeys,
    pub height: i64,
    #[serde(with = "crate::lotus_json")]
    pub message: Cid,
    pub return_dec: IpldJson,
}

// Net API
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct AddrInfo {
    #[serde(rename = "ID")]
    pub id: String,
    pub addrs: HashSet<Multiaddr>,
}

lotus_json_with_self!(AddrInfo);

#[derive(Serialize, Deserialize)]
pub struct PeerID {
    pub multihash: Multihash,
}

/// Represents the current version of the API.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct APIVersion {
    pub version: String,
    #[serde(rename = "APIVersion")]
    pub api_version: Version,
    pub block_delay: u64,
}

lotus_json_with_self!(APIVersion);

/// Integer based value on version information. Highest order bits for Major,
/// Mid order for Minor and lowest for Patch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Version(u32);

impl Version {
    pub const fn new(major: u64, minor: u64, patch: u64) -> Self {
        Self((major as u32) << 16 | (minor as u32) << 8 | (patch as u32))
    }
}

#[derive(Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct ApiMessage {
    cid: Cid,
    message: Message,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ApiMessageLotusJson {
    cid: LotusJson<Cid>,
    message: LotusJson<Message>,
}

impl HasLotusJson for ApiMessage {
    type LotusJson = ApiMessageLotusJson;
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![]
    }
    fn into_lotus_json(self) -> Self::LotusJson {
        ApiMessageLotusJson {
            cid: LotusJson(self.cid),
            message: LotusJson(self.message),
        }
    }
    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        ApiMessage {
            cid: lotus_json.cid.into_inner(),
            message: lotus_json.message.into_inner(),
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct MinerPowerLotusJson {
    miner_power: LotusJson<Claim>,
    total_power: LotusJson<Claim>,
    has_min_power: bool,
}

impl HasLotusJson for MinerPower {
    type LotusJson = MinerPowerLotusJson;
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![]
    }
    fn into_lotus_json(self) -> Self::LotusJson {
        MinerPowerLotusJson {
            miner_power: LotusJson(self.miner_power),
            total_power: LotusJson(self.total_power),
            has_min_power: self.has_min_power,
        }
    }
    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        MinerPower {
            miner_power: lotus_json.miner_power.into_inner(),
            total_power: lotus_json.total_power.into_inner(),
            has_min_power: lotus_json.has_min_power,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DiscoverResult {
    info: DiscoverInfo,
    methods: Vec<DiscoverMethod>,
    openrpc: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoverMethod {
    deprecated: bool,
    description: String,
    external_docs: DiscoverDocs,
    name: String,
    param_structure: String,
    params: Value,
    // Missing 'result' field. Tracking issue:
    // https://github.com/ChainSafe/forest/issues/3585
    summary: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DiscoverDocs {
    description: String,
    url: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DiscoverInfo {
    title: String,
    version: String,
}

lotus_json_with_self!(DiscoverResult, DiscoverMethod, DiscoverDocs, DiscoverInfo);

#[derive(Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct ApiActorState {
    #[serde(with = "crate::lotus_json")]
    balance: TokenAmount,
    #[serde(with = "crate::lotus_json")]
    code: Cid,
    #[serde(with = "crate::lotus_json")]
    state: Ipld,
}

lotus_json_with_self!(ApiActorState);
