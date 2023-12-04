// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::str::FromStr;
use std::sync::Arc;

use crate::beacon::BeaconSchedule;
use crate::blocks::TipsetKeys;
use crate::chain::ChainStore;
use crate::chain_sync::{BadBlockCache, SyncState};
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
    error::ExitCode,
    executor::Receipt,
    message::Message,
    sector::{RegisteredSealProof, SectorNumber},
    state_tree::ActorState,
};
use crate::state_manager::StateManager;
use ahash::HashSet;
use chrono::Utc;
use cid::Cid;
use fil_actor_interface::miner::MinerInfo;
use fil_actor_interface::{
    market::{DealProposal, DealState},
    miner::MinerPower,
    power::Claim,
};
use fil_actor_miner_state::v12::{BeneficiaryTerm, PendingBeneficiaryChange};
use fil_actors_shared::fvm_ipld_bitfield::BitField;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::{BytesDe, RawBytes};
use jsonrpc_v2::{MapRouter as JsonRpcMapRouter, Server as JsonRpcServer};
use libipld_core::ipld::Ipld;
use libp2p::PeerId;
use num_bigint::BigInt;
use parking_lot::RwLock as SyncRwLock;
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
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
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct BlockMessages {
    #[serde(rename = "BlsMessages", with = "crate::lotus_json")]
    pub bls_msg: Vec<Message>,
    #[serde(rename = "SecpkMessages", with = "crate::lotus_json")]
    pub secp_msg: Vec<SignedMessage>,
    #[serde(rename = "Cids", with = "crate::lotus_json")]
    pub cids: Vec<Cid>,
}

lotus_json_with_self!(BlockMessages);

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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct MessageLookup {
    #[serde(with = "crate::lotus_json")]
    pub receipt: Receipt,
    #[serde(rename = "TipSet", with = "crate::lotus_json")]
    pub tipset: TipsetKeys,
    pub height: i64,
    #[serde(with = "crate::lotus_json")]
    pub message: Cid,
    // #[serde(with = "crate::lotus_json")]
    #[serde(skip)]
    pub return_dec: Option<Ipld>,
}

lotus_json_with_self!(MessageLookup);

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

const EMPTY_ADDRESS_VALUE: &str = "<empty>";

/// This wrapper is needed because of a bug in Lotus.
/// See: <https://github.com/filecoin-project/lotus/issues/11461>.
#[derive(Debug, Eq, PartialEq, Clone)]
pub struct AddressOrEmpty(pub Option<Address>);

impl Serialize for AddressOrEmpty {
    fn serialize<S>(&self, s: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let address_bytes = match self.0 {
            Some(addr) => addr.to_string(),
            None => EMPTY_ADDRESS_VALUE.to_string(),
        };

        s.collect_str(&address_bytes)
    }
}

impl<'de> Deserialize<'de> for AddressOrEmpty {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let address_str = String::deserialize(deserializer)?;
        if address_str.eq(EMPTY_ADDRESS_VALUE) {
            return Ok(Self(None));
        }

        Address::from_str(&address_str)
            .map_err(de::Error::custom)
            .map(|addr| Self(Some(addr)))
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct MinerInfoLotusJson {
    #[serde(with = "crate::lotus_json")]
    pub owner: Address,
    #[serde(with = "crate::lotus_json")]
    pub worker: Address,
    pub new_worker: AddressOrEmpty,
    #[serde(with = "crate::lotus_json")]
    pub control_addresses: Vec<Address>, // Must all be ID addresses.
    pub worker_change_epoch: ChainEpoch,
    //#[serde(with = "crate::lotus_json")]
    pub peer_id: Option<PeerId>,
    #[serde(with = "crate::lotus_json")]
    pub multiaddrs: Vec<Vec<u8>>,
    pub window_po_st_proof_type: fvm_shared2::sector::RegisteredPoStProof,
    pub sector_size: fvm_shared2::sector::SectorSize,
    pub window_po_st_partition_sectors: u64,
    pub consensus_fault_elapsed: ChainEpoch,
    #[serde(with = "crate::lotus_json")]
    pub pending_owner_address: Option<Address>,
    #[serde(with = "crate::lotus_json")]
    pub beneficiary: Address,
    #[serde(with = "crate::lotus_json")]
    pub beneficiary_term: BeneficiaryTerm,
    #[serde(with = "crate::lotus_json")]
    pub pending_beneficiary_term: Option<PendingBeneficiaryChange>,
}

impl HasLotusJson for MinerInfo {
    type LotusJson = MinerInfoLotusJson;
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![]
    }
    fn into_lotus_json(self) -> Self::LotusJson {
        MinerInfoLotusJson {
            owner: self.owner.into(),
            worker: self.worker.into(),
            new_worker: AddressOrEmpty(self.new_worker.map(|addr| addr.into())),
            control_addresses: self
                .control_addresses
                .into_iter()
                .map(|a| a.into())
                .collect(),
            worker_change_epoch: self.worker_change_epoch,
            peer_id: PeerId::try_from(self.peer_id).ok(),
            multiaddrs: self.multiaddrs.into_iter().map(|addr| addr.0).collect(),
            window_po_st_proof_type: self.window_post_proof_type,
            sector_size: self.sector_size,
            window_po_st_partition_sectors: self.window_post_partition_sectors,
            consensus_fault_elapsed: self.consensus_fault_elapsed,
            // NOTE: In Lotus this field is never set for any of the versions, so we have to ignore
            // it too.
            // See: <https://github.com/filecoin-project/lotus/blob/b6a77dfafcf0110e95840fca15a775ed663836d8/chain/actors/builtin/miner/v12.go#L370>.
            pending_owner_address: None,
            beneficiary: self.beneficiary.into(),
            beneficiary_term: self.beneficiary_term,
            pending_beneficiary_term: self.pending_beneficiary_term,
        }
    }
    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        MinerInfo {
            owner: lotus_json.owner.into(),
            worker: lotus_json.worker.into(),
            new_worker: lotus_json.new_worker.0.map(|addr| addr.into()),
            control_addresses: lotus_json
                .control_addresses
                .into_iter()
                .map(|a| a.into())
                .collect(),
            worker_change_epoch: lotus_json.worker_change_epoch,
            peer_id: lotus_json.peer_id.unwrap_or(PeerId::random()).to_bytes(),
            multiaddrs: lotus_json.multiaddrs.into_iter().map(BytesDe).collect(),
            window_post_proof_type: lotus_json.window_po_st_proof_type,
            sector_size: lotus_json.sector_size,
            window_post_partition_sectors: lotus_json.window_po_st_partition_sectors,
            consensus_fault_elapsed: lotus_json.consensus_fault_elapsed,
            // Ignore this field as it is never set on Lotus side.
            pending_owner_address: None,
            beneficiary: lotus_json.beneficiary.into(),
            beneficiary_term: lotus_json.beneficiary_term,
            pending_beneficiary_term: lotus_json.pending_beneficiary_term,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct BeneficiaryTermLotusJson {
    /// The total amount the current beneficiary can withdraw. Monotonic, but reset when beneficiary changes.
    #[serde(with = "crate::lotus_json")]
    pub quota: TokenAmount,
    /// The amount of quota the current beneficiary has already withdrawn
    #[serde(with = "crate::lotus_json")]
    pub used_quota: TokenAmount,
    /// The epoch at which the beneficiary's rights expire and revert to the owner
    pub expiration: ChainEpoch,
}

impl HasLotusJson for BeneficiaryTerm {
    type LotusJson = BeneficiaryTermLotusJson;

    fn snapshots() -> Vec<(Value, Self)> {
        vec![]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        BeneficiaryTermLotusJson {
            used_quota: self.used_quota.into(),
            quota: self.quota.into(),
            expiration: self.expiration,
        }
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        Self {
            used_quota: lotus_json.used_quota.into(),
            quota: lotus_json.quota.into(),
            expiration: lotus_json.expiration,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct PendingBeneficiaryChangeLotusJson {
    #[serde(with = "crate::lotus_json")]
    pub new_beneficiary: Address,
    #[serde(with = "crate::lotus_json")]
    pub new_quota: TokenAmount,
    pub new_expiration: ChainEpoch,
    pub approved_by_beneficiary: bool,
    pub approved_by_nominee: bool,
}

impl HasLotusJson for PendingBeneficiaryChange {
    type LotusJson = PendingBeneficiaryChangeLotusJson;

    fn snapshots() -> Vec<(Value, Self)> {
        vec![]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        PendingBeneficiaryChangeLotusJson {
            new_beneficiary: self.new_beneficiary.into(),
            new_quota: self.new_quota.into(),
            new_expiration: self.new_expiration,
            approved_by_beneficiary: self.approved_by_beneficiary,
            approved_by_nominee: self.approved_by_nominee,
        }
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        Self {
            new_beneficiary: lotus_json.new_beneficiary.into(),
            new_quota: lotus_json.new_quota.into(),
            new_expiration: lotus_json.new_expiration,
            approved_by_beneficiary: lotus_json.approved_by_beneficiary,
            approved_by_nominee: lotus_json.approved_by_nominee,
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
#[derive(Serialize, Deserialize, PartialEq, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct ApiActorState {
    #[serde(with = "crate::lotus_json")]
    balance: TokenAmount,
    #[serde(with = "crate::lotus_json")]
    code: Cid,
    #[serde(with = "crate::lotus_json")]
    state: ApiState,
}

#[derive(Serialize, Deserialize, PartialEq, Clone)]
#[serde(rename_all = "PascalCase")]
struct ApiState {
    #[serde(rename = "BuiltinActors")]
    #[serde(with = "crate::lotus_json")]
    state: Ipld,
}

lotus_json_with_self!(ApiState);
lotus_json_with_self!(ApiActorState);

impl ApiActorState {
    pub fn new(balance: TokenAmount, code: Cid, state: Ipld) -> Self {
        Self {
            balance,
            code,
            state: ApiState { state },
        }
    }
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
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

impl From<fil_actor_interface::miner::SectorOnChainInfo> for SectorOnChainInfo {
    fn from(other: fil_actor_interface::miner::SectorOnChainInfo) -> Self {
        SectorOnChainInfo {
            sector_number: other.sector_number,
            seal_proof: other.seal_proof.into(),
            sealed_cid: other.sealed_cid,
            deal_ids: other.deal_ids,
            activation: other.activation,
            expiration: other.expiration,
            deal_weight: other.deal_weight,
            verified_deal_weight: other.verified_deal_weight,
            initial_pledge: other.initial_pledge.into(),
            expected_day_reward: other.expected_day_reward.into(),
            expected_storage_pledge: other.expected_storage_pledge.into(),
            replaced_sector_age: other.replaced_sector_age,
            // `replaced_day_reward` has to be zero and Lemmih cannot figure out
            // why. Lotus casts all `SectorOnChainInfo` structs to the miner-v9
            // version which clears some fields (like `simple_qa_power`) but it
            // shouldn't clear `replaced_day_reward`. Oh well, maybe one day
            // Lemmih will figure it out.
            replaced_day_reward: TokenAmount::default(),
            sector_key_cid: other.sector_key_cid,
            simple_qa_power: other.simple_qa_power,
        }
    }
}

lotus_json_with_self!(SectorOnChainInfo);

#[derive(Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct ApiDeadline {
    #[serde(with = "crate::lotus_json")]
    pub post_submissions: BitField,
    #[serde(with = "crate::lotus_json")]
    pub disputable_proof_count: u64,
}

lotus_json_with_self!(ApiDeadline);
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ApiInvocResult {
    #[serde(with = "crate::lotus_json")]
    pub msg: Message,
    #[serde(with = "crate::lotus_json")]
    pub msg_cid: Cid,
    #[serde(with = "crate::lotus_json")]
    pub msg_rct: Option<Receipt>,
    pub error: String,
    pub duration: u64,
    #[serde(with = "crate::lotus_json")]
    pub gas_cost: MessageGasCost,
    #[serde(with = "crate::lotus_json")]
    pub execution_trace: Option<ExecutionTrace>,
}

lotus_json_with_self!(ApiInvocResult);

impl PartialEq for ApiInvocResult {
    /// Ignore [`Self::duration`] as it is implementation-dependent
    fn eq(&self, other: &Self) -> bool {
        self.msg == other.msg
            && self.msg_cid == other.msg_cid
            && self.msg_rct == other.msg_rct
            && self.error == other.error
            && self.gas_cost == other.gas_cost
            && self.execution_trace == other.execution_trace
    }
}

#[derive(Default, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct MessageGasCost {
    #[serde(with = "crate::lotus_json")]
    pub message: Option<Cid>,
    #[serde(with = "crate::lotus_json")]
    pub gas_used: TokenAmount,
    #[serde(with = "crate::lotus_json")]
    pub base_fee_burn: TokenAmount,
    #[serde(with = "crate::lotus_json")]
    pub over_estimation_burn: TokenAmount,
    #[serde(with = "crate::lotus_json")]
    pub miner_penalty: TokenAmount,
    #[serde(with = "crate::lotus_json")]
    pub miner_tip: TokenAmount,
    #[serde(with = "crate::lotus_json")]
    pub refund: TokenAmount,
    #[serde(with = "crate::lotus_json")]
    pub total_cost: TokenAmount,
}

lotus_json_with_self!(MessageGasCost);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ExecutionTrace {
    #[serde(with = "crate::lotus_json")]
    pub msg: MessageTrace,
    #[serde(with = "crate::lotus_json")]
    pub msg_rct: ReturnTrace,
    #[serde(with = "crate::lotus_json")]
    pub gas_charges: Vec<GasTrace>,
    #[serde(with = "crate::lotus_json")]
    pub subcalls: Vec<ExecutionTrace>,
}

lotus_json_with_self!(ExecutionTrace);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct MessageTrace {
    #[serde(with = "crate::lotus_json")]
    pub from: Address,
    #[serde(with = "crate::lotus_json")]
    pub to: Address,
    #[serde(with = "crate::lotus_json")]
    pub value: TokenAmount,
    pub method: u64,
    #[serde(with = "crate::lotus_json")]
    pub params: RawBytes,
    pub params_codec: u64,
    pub gas_limit: Option<u64>,
    pub read_only: Option<bool>,
    #[serde(with = "crate::lotus_json")]
    pub code_cid: Cid,
}

lotus_json_with_self!(MessageTrace);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ReturnTrace {
    pub exit_code: ExitCode,
    #[serde(with = "crate::lotus_json")]
    pub r#return: RawBytes,
    pub return_codec: u64,
}

lotus_json_with_self!(ReturnTrace);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct GasTrace {
    pub name: String,
    #[serde(rename = "tg")]
    pub total_gas: u64,
    #[serde(rename = "cg")]
    pub compute_gas: u64,
    #[serde(rename = "sg")]
    pub storage_gas: u64,
    #[serde(rename = "tt")]
    pub time_taken: u64,
}

lotus_json_with_self!(GasTrace);

impl PartialEq for GasTrace {
    /// Ignore [`Self::total_gas`] as it is implementation-dependent
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
            && self.total_gas == other.total_gas
            && self.compute_gas == other.compute_gas
            && self.storage_gas == other.storage_gas
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct CirculatingSupply {
    #[serde(with = "crate::lotus_json")]
    pub fil_vested: TokenAmount,
    #[serde(with = "crate::lotus_json")]
    pub fil_mined: TokenAmount,
    #[serde(with = "crate::lotus_json")]
    pub fil_burnt: TokenAmount,
    #[serde(with = "crate::lotus_json")]
    pub fil_locked: TokenAmount,
    #[serde(with = "crate::lotus_json")]
    pub fil_circulating: TokenAmount,
    #[serde(with = "crate::lotus_json")]
    pub fil_reserve_disbursed: TokenAmount,
}

lotus_json_with_self!(CirculatingSupply);
