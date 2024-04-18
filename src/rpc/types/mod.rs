// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Types that are shared _between_ APIs.
//!
//! If a type here is used by only one API, it should be relocated.

mod address_impl;
mod beneficiary_impl;
mod deal_impl;
mod miner_impl;
mod sector_impl;
mod tsk_impl;

#[cfg(test)]
mod tests;

use crate::beacon::BeaconEntry;
use crate::blocks::TipsetKey;
use crate::libp2p::Multihash;
use crate::lotus_json::{lotus_json_with_self, HasLotusJson, LotusJson};
use crate::shim::sector::SectorInfo;
use crate::shim::{
    address::Address,
    clock::ChainEpoch,
    deal::DealID,
    econ::TokenAmount,
    executor::Receipt,
    fvm_shared_latest::MethodNum,
    message::Message,
    sector::{RegisteredSealProof, SectorNumber},
};
use cid::Cid;
use fil_actor_interface::market::AllocationID;
use fil_actor_interface::miner::MinerInfo;
use fil_actor_interface::{
    market::{DealProposal, DealState},
    miner::MinerPower,
    power::Claim,
};
use fil_actor_miner_state::v12::{BeneficiaryTerm, PendingBeneficiaryChange};
use fil_actors_shared::fvm_ipld_bitfield::BitField;
use fvm_ipld_encoding::{BytesDe, RawBytes};
use libipld_core::ipld::Ipld;
use libp2p::PeerId;
use nonempty::NonEmpty;
use num_bigint::BigInt;
use schemars::JsonSchema;
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
#[cfg(test)]
use serde_json::Value;
use std::str::FromStr;

/// Lotus de/serializes empty Arrays as `null`.
///
/// This should be preferred over [`LotusJson<Vec<T>>`]
pub struct NullableVec<T>(pub Vec<T>);

impl<T> Serialize for NullableVec<T>
where
    T: Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let v = &self.0;
        match v.is_empty() {
            true => None,
            false => Some(v),
        }
        .serialize(serializer)
    }
}

// Chain API

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub struct MessageSendSpec {
    max_fee: LotusJson<TokenAmount>,
}

lotus_json_with_self!(MessageSendSpec);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct ApiDealState {
    pub sector_start_epoch: ChainEpoch,
    pub last_updated_epoch: ChainEpoch,
    pub slash_epoch: ChainEpoch,
    #[serde(skip)]
    pub verified_claim: AllocationID,
}

lotus_json_with_self!(ApiDealState);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct ApiDealProposal {
    #[serde(rename = "PieceCID", with = "crate::lotus_json")]
    pub piece_cid: Cid,
    pub piece_size: u64,
    pub verified_deal: bool,
    #[serde(with = "crate::lotus_json")]
    pub client: Address,
    #[serde(with = "crate::lotus_json")]
    pub provider: Address,
    pub label: String,
    pub start_epoch: ChainEpoch,
    pub end_epoch: ChainEpoch,
    #[serde(with = "crate::lotus_json")]
    pub storage_price_per_epoch: TokenAmount,
    #[serde(with = "crate::lotus_json")]
    pub provider_collateral: TokenAmount,
    #[serde(with = "crate::lotus_json")]
    pub client_collateral: TokenAmount,
}

lotus_json_with_self!(ApiDealProposal);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct ApiMarketDeal {
    #[serde(with = "crate::lotus_json")]
    pub proposal: ApiDealProposal,
    #[serde(with = "crate::lotus_json")]
    pub state: ApiDealState,
}

lotus_json_with_self!(ApiMarketDeal);

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
    pub tipset: TipsetKey,
    pub height: i64,
    #[serde(with = "crate::lotus_json")]
    pub message: Cid,
    #[serde(with = "crate::lotus_json")]
    pub return_dec: Ipld,
}

lotus_json_with_self!(MessageLookup);

#[derive(Serialize, Deserialize)]
pub struct PeerID {
    pub multihash: Multihash,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
pub struct ApiTipsetKey(pub Option<TipsetKey>);

/// This wrapper is needed because of a bug in Lotus.
/// See: <https://github.com/filecoin-project/lotus/issues/11461>.
#[derive(Debug, Eq, PartialEq, Clone)]
pub struct AddressOrEmpty(pub Option<Address>);

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
    #[serde(with = "crate::lotus_json")]
    pub peer_id: Option<String>,
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

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct MinerPowerLotusJson {
    miner_power: LotusJson<Claim>,
    total_power: LotusJson<Claim>,
    has_min_power: bool,
}

// Note: kept the name in line with Lotus implementation for cross-referencing simplicity.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct MiningBaseInfo {
    #[serde(with = "crate::lotus_json")]
    pub miner_power: crate::shim::sector::StoragePower,
    #[serde(with = "crate::lotus_json")]
    pub network_power: fvm_shared2::sector::StoragePower,
    #[serde(with = "crate::lotus_json")]
    pub sectors: Vec<SectorInfo>,
    #[serde(with = "crate::lotus_json")]
    pub worker_key: Address,
    pub sector_size: fvm_shared2::sector::SectorSize,
    #[serde(with = "crate::lotus_json")]
    pub prev_beacon_entry: BeaconEntry,
    #[serde(with = "crate::lotus_json")]
    pub beacon_entries: Vec<BeaconEntry>,
    pub eligible_for_mining: bool,
}

lotus_json_with_self!(MiningBaseInfo);

#[derive(Serialize, Deserialize, PartialEq, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct ApiActorState {
    #[serde(with = "crate::lotus_json")]
    pub balance: TokenAmount,
    #[serde(with = "crate::lotus_json")]
    pub code: Cid,
    pub state: ApiState,
}

lotus_json_with_self!(ApiActorState);

#[derive(Serialize, Deserialize, PartialEq, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct ApiState {
    #[serde(with = "crate::lotus_json")]
    pub builtin_actors: Ipld,
}

lotus_json_with_self!(ApiState);

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub struct SectorOnChainInfo {
    pub sector_number: SectorNumber,

    #[schemars(with = "i64")]
    /// The seal proof type implies the PoSt proofs
    pub seal_proof: RegisteredSealProof,

    #[schemars(with = "LotusJson<Cid>")]
    #[serde(with = "crate::lotus_json", rename = "SealedCID")]
    /// `CommR`
    pub sealed_cid: Cid,

    #[schemars(with = "LotusJson<Vec<DealID>>")]
    #[serde(with = "crate::lotus_json", rename = "DealIDs")]
    pub deal_ids: Vec<DealID>,

    /// Epoch during which the sector proof was accepted
    pub activation: ChainEpoch,

    /// Epoch during which the sector expires
    pub expiration: ChainEpoch,

    #[schemars(with = "LotusJson<BigInt>")]
    #[serde(with = "crate::lotus_json")]
    /// Integral of active deals over sector lifetime
    pub deal_weight: BigInt,

    #[schemars(with = "LotusJson<BigInt>")]
    #[serde(with = "crate::lotus_json")]
    /// Integral of active verified deals over sector lifetime
    pub verified_deal_weight: BigInt,

    #[schemars(with = "LotusJson<TokenAmount>")]
    #[serde(with = "crate::lotus_json")]
    /// Pledge collected to commit this sector
    pub initial_pledge: TokenAmount,

    #[schemars(with = "LotusJson<TokenAmount>")]
    #[serde(with = "crate::lotus_json")]
    /// Expected one day projection of reward for sector computed at activation
    /// time
    pub expected_day_reward: TokenAmount,

    #[schemars(with = "LotusJson<TokenAmount>")]
    #[serde(with = "crate::lotus_json")]
    /// Expected twenty day projection of reward for sector computed at
    /// activation time
    pub expected_storage_pledge: TokenAmount,

    pub replaced_sector_age: ChainEpoch,

    #[schemars(with = "LotusJson<TokenAmount>")]
    #[serde(with = "crate::lotus_json")]
    pub replaced_day_reward: TokenAmount,

    #[schemars(with = "LotusJson<Option<Cid>>")]
    #[serde(with = "crate::lotus_json", rename = "SectorKeyCID")]
    pub sector_key_cid: Option<Cid>,

    #[serde(rename = "SimpleQAPower")]
    pub simple_qa_power: bool,
}

lotus_json_with_self!(SectorOnChainInfo);

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub struct SectorPreCommitOnChainInfo {
    #[schemars(with = "LotusJson<SectorPreCommitInfo>")]
    #[serde(with = "crate::lotus_json")]
    pub info: SectorPreCommitInfo,
    #[schemars(with = "LotusJson<TokenAmount>")]
    #[serde(with = "crate::lotus_json")]
    pub pre_commit_deposit: TokenAmount,
    pub pre_commit_epoch: ChainEpoch,
}

lotus_json_with_self!(SectorPreCommitOnChainInfo);

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub struct SectorPreCommitInfo {
    #[schemars(with = "LotusJson<RegisteredSealProof>")]
    #[serde(with = "crate::lotus_json")]
    pub seal_proof: RegisteredSealProof,
    pub sector_number: SectorNumber,
    #[schemars(with = "LotusJson<Cid>")]
    #[serde(rename = "SealedCID", with = "crate::lotus_json")]
    pub sealed_cid: Cid,
    pub seal_rand_epoch: ChainEpoch,
    #[schemars(with = "LotusJson<Vec<DealID>>")]
    #[serde(rename = "DealIDs", with = "crate::lotus_json")]
    pub deal_ids: Vec<DealID>,
    pub expiration: ChainEpoch,
    #[schemars(with = "LotusJson<Option<Cid>>")]
    #[serde(with = "crate::lotus_json")]
    pub unsealed_cid: Option<Cid>,
}

lotus_json_with_self!(SectorPreCommitInfo);

#[derive(Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct ApiDeadline {
    #[serde(with = "crate::lotus_json")]
    pub post_submissions: BitField,
    #[serde(with = "crate::lotus_json")]
    pub disputable_proof_count: u64,
}

lotus_json_with_self!(ApiDeadline);

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

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct MinerSectors {
    live: u64,
    active: u64,
    faulty: u64,
}

impl MinerSectors {
    pub fn new(live: u64, active: u64, faulty: u64) -> Self {
        Self {
            live,
            active,
            faulty,
        }
    }
}

lotus_json_with_self!(MinerSectors);

#[derive(Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct MinerPartitions {
    #[serde(with = "crate::lotus_json")]
    all_sectors: BitField,
    #[serde(with = "crate::lotus_json")]
    faulty_sectors: BitField,
    #[serde(with = "crate::lotus_json")]
    recovering_sectors: BitField,
    #[serde(with = "crate::lotus_json")]
    live_sectors: BitField,
    #[serde(with = "crate::lotus_json")]
    active_sectors: BitField,
}

impl MinerPartitions {
    pub fn new(
        all_sectors: &BitField,
        faulty_sectors: &BitField,
        recovering_sectors: &BitField,
        live_sectors: BitField,
        active_sectors: BitField,
    ) -> Self {
        Self {
            all_sectors: all_sectors.clone(),
            faulty_sectors: faulty_sectors.clone(),
            recovering_sectors: recovering_sectors.clone(),
            live_sectors,
            active_sectors,
        }
    }
}

lotus_json_with_self!(MinerPartitions);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub struct MessageFilter {
    #[schemars(with = "LotusJson<Option<Address>>")]
    #[serde(with = "crate::lotus_json")]
    pub from: Option<Address>,

    #[schemars(with = "LotusJson<Option<Address>>")]
    #[serde(with = "crate::lotus_json")]
    pub to: Option<Address>,
}

impl MessageFilter {
    pub fn matches(&self, msg: &Message) -> bool {
        if let Some(from) = &self.from {
            if from != &msg.from {
                return false;
            }
        }

        if let Some(to) = &self.to {
            if to != &msg.to {
                return false;
            }
        }

        true
    }

    pub fn is_empty(&self) -> bool {
        self.from.is_none() && self.to.is_none()
    }
}

lotus_json_with_self!(MessageFilter);

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Transaction {
    #[serde(rename = "ID")]
    pub id: i64,
    #[serde(with = "crate::lotus_json")]
    pub to: Address,
    #[serde(with = "crate::lotus_json")]
    pub value: TokenAmount,
    pub method: MethodNum,
    #[serde(with = "crate::lotus_json")]
    pub params: RawBytes,
    #[serde(with = "crate::lotus_json")]
    pub approved: Vec<Address>,
}

lotus_json_with_self!(Transaction);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct DealCollateralBounds {
    #[serde(with = "crate::lotus_json")]
    pub min: TokenAmount,
    #[serde(with = "crate::lotus_json")]
    pub max: TokenAmount,
}

lotus_json_with_self!(DealCollateralBounds);
