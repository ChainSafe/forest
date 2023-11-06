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
use crate::lotus_json::{lotus_json_with_self, HasLotusJson, LotusJson};
use crate::message::signed_message::SignedMessage;
use crate::message_pool::{MessagePool, MpoolRpcProvider};
use crate::shim::{
    address::Address,
    clock::ChainEpoch,
    deal::DealID,
    econ::TokenAmount,
    executor::Receipt,
    message::Message,
    sector::{RegisteredSealProof, SectorNumber},
    state_tree::ActorState,
};
use crate::state_manager::StateManager;
use ahash::HashSet;
use chrono::Utc;
use cid::Cid;
use fil_actor_interface::{
    market::{DealProposal, DealState},
    miner::MinerPower,
    power::Claim,
};
use fvm_ipld_blockstore::Blockstore;
use jsonrpc_v2::{MapRouter as JsonRpcMapRouter, Server as JsonRpcServer};
use libipld_core::ipld::Ipld;
use num_bigint::BigInt;
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

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct ApiMessage {
    cid: Cid,
    message: Message,
}

impl ApiMessage {
    pub fn new(cid: Cid, message: Message) -> Self {
        Self { cid, message }
    }
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

/// State of all actor implementations.
#[derive(PartialEq, Eq, Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ActorStateJson {
    #[serde(with = "crate::lotus_json")]
    /// Link to code for the actor.
    pub code: Cid,
    #[serde(with = "crate::lotus_json")]
    /// Link to the state of the actor.
    pub head: Cid,
    /// Sequence of the actor.
    pub nonce: u64,
    #[serde(with = "crate::lotus_json")]
    /// Tokens available to the actor.
    pub balance: TokenAmount,
    #[serde(with = "crate::lotus_json")]
    /// The actor's "delegated" address, if assigned.
    /// This field is set on actor creation and never modified.
    pub address: Option<Address>,
}

impl HasLotusJson for ActorState {
    type LotusJson = ActorStateJson;
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![]
    }
    fn into_lotus_json(self) -> Self::LotusJson {
        ActorStateJson {
            code: self.code,
            head: self.state,
            nonce: self.sequence,
            balance: self.balance.clone().into(),
            address: self.delegated_address.map(|a| a.into()),
        }
    }
    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        ActorState::new(
            lotus_json.code,
            lotus_json.head,
            lotus_json.balance,
            lotus_json.nonce,
            lotus_json.address,
        )
    }
}
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

#[derive(Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct SectorOnChainInfo {
    pub sector_number: SectorNumber,

    /// The seal proof type implies the PoSt proofs
    pub seal_proof: RegisteredSealProof,

    #[serde(with = "crate::lotus_json")]
    #[serde(rename = "SealedCID")]
    /// `CommR`
    pub sealed_cid: Cid,

    #[serde(rename = "DealIDs")]
    #[serde(with = "crate::lotus_json")]
    pub deal_ids: Vec<DealID>,

    /// Epoch during which the sector proof was accepted
    pub activation: ChainEpoch,

    /// Epoch during which the sector expires
    pub expiration: ChainEpoch,

    #[serde(with = "crate::lotus_json")]
    /// Integral of active deals over sector lifetime
    pub deal_weight: BigInt,

    #[serde(with = "crate::lotus_json")]
    /// Integral of active verified deals over sector lifetime
    pub verified_deal_weight: BigInt,

    #[serde(with = "crate::lotus_json")]
    /// Pledge collected to commit this sector
    pub initial_pledge: TokenAmount,

    #[serde(with = "crate::lotus_json")]
    /// Expected one day projection of reward for sector computed at activation
    /// time
    pub expected_day_reward: TokenAmount,

    #[serde(with = "crate::lotus_json")]
    /// Expected twenty day projection of reward for sector computed at
    /// activation time
    pub expected_storage_pledge: TokenAmount,

    pub replaced_sector_age: ChainEpoch,

    #[serde(with = "crate::lotus_json")]
    pub replaced_day_reward: TokenAmount,

    #[serde(with = "crate::lotus_json")]
    #[serde(rename = "SectorKeyCID")]
    pub sector_key_cid: Option<Cid>,

    #[serde(rename = "SimpleQAPower")]
    pub simple_qa_power: bool,
}

lotus_json_with_self!(SectorOnChainInfo);
