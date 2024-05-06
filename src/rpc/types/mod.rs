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

use crate::blocks::TipsetKey;
use crate::libp2p::Multihash;
use crate::lotus_json::{lotus_json_with_self, HasLotusJson, LotusJson};
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
use fil_actor_interface::miner::{DeadlineInfo, MinerInfo};
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
use num_bigint::BigInt;
use nunny::Vec as NonEmpty;
use schemars::JsonSchema;
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
#[cfg(test)]
use serde_json::Value;
use std::str::FromStr;

// Chain API

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub struct MessageSendSpec {
    #[schemars(with = "LotusJson<TokenAmount>")]
    #[serde(with = "crate::lotus_json")]
    max_fee: TokenAmount,
}

lotus_json_with_self!(MessageSendSpec);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub struct ApiDealState {
    pub sector_start_epoch: ChainEpoch,
    pub last_updated_epoch: ChainEpoch,
    pub slash_epoch: ChainEpoch,
    #[serde(skip)]
    pub verified_claim: AllocationID,
}

lotus_json_with_self!(ApiDealState);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub struct ApiDealProposal {
    #[schemars(with = "LotusJson<Cid>")]
    #[serde(rename = "PieceCID", with = "crate::lotus_json")]
    pub piece_cid: Cid,
    pub piece_size: u64,
    pub verified_deal: bool,
    #[schemars(with = "LotusJson<Address>")]
    #[serde(with = "crate::lotus_json")]
    pub client: Address,
    #[schemars(with = "LotusJson<Address>")]
    #[serde(with = "crate::lotus_json")]
    pub provider: Address,
    pub label: String,
    pub start_epoch: ChainEpoch,
    pub end_epoch: ChainEpoch,
    #[schemars(with = "LotusJson<TokenAmount>")]
    #[serde(with = "crate::lotus_json")]
    pub storage_price_per_epoch: TokenAmount,
    #[schemars(with = "LotusJson<TokenAmount>")]
    #[serde(with = "crate::lotus_json")]
    pub provider_collateral: TokenAmount,
    #[schemars(with = "LotusJson<TokenAmount>")]
    #[serde(with = "crate::lotus_json")]
    pub client_collateral: TokenAmount,
}

lotus_json_with_self!(ApiDealProposal);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub struct ApiMarketDeal {
    pub proposal: ApiDealProposal,
    pub state: ApiDealState,
}

lotus_json_with_self!(ApiMarketDeal);

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct MarketDeal {
    pub proposal: DealProposal,
    pub state: DealState,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub struct MessageLookup {
    #[schemars(with = "LotusJson<Receipt>")]
    #[serde(with = "crate::lotus_json")]
    pub receipt: Receipt,
    #[schemars(with = "LotusJson<TipsetKey>")]
    #[serde(rename = "TipSet", with = "crate::lotus_json")]
    pub tipset: TipsetKey,
    pub height: i64,
    #[schemars(with = "LotusJson<Cid>")]
    #[serde(with = "crate::lotus_json")]
    pub message: Cid,
    #[schemars(with = "serde_json::Value")]
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

#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub struct MinerInfoLotusJson {
    #[schemars(with = "LotusJson<Address>")]
    #[serde(with = "crate::lotus_json")]
    pub owner: Address,
    #[schemars(with = "LotusJson<Address>")]
    #[serde(with = "crate::lotus_json")]
    pub worker: Address,
    #[schemars(with = "LotusJson<Option<Address>>")]
    pub new_worker: AddressOrEmpty,
    #[schemars(with = "LotusJson<Vec<Address>>")]
    #[serde(with = "crate::lotus_json")]
    pub control_addresses: Vec<Address>, // Must all be ID addresses.
    pub worker_change_epoch: ChainEpoch,
    #[schemars(with = "LotusJson<Option<String>>")]
    #[serde(with = "crate::lotus_json")]
    pub peer_id: Option<String>,
    #[schemars(with = "LotusJson<Vec<Vec<u8>>>")]
    #[serde(with = "crate::lotus_json")]
    pub multiaddrs: Vec<Vec<u8>>,
    #[schemars(with = "String")]
    pub window_po_st_proof_type: fvm_shared2::sector::RegisteredPoStProof,
    #[schemars(with = "u64")]
    pub sector_size: fvm_shared2::sector::SectorSize,
    pub window_po_st_partition_sectors: u64,
    pub consensus_fault_elapsed: ChainEpoch,
    #[schemars(with = "LotusJson<Option<Address>>")]
    #[serde(with = "crate::lotus_json")]
    pub pending_owner_address: Option<Address>,
    #[schemars(with = "LotusJson<Address>")]
    #[serde(with = "crate::lotus_json")]
    pub beneficiary: Address,
    #[schemars(with = "LotusJson<BeneficiaryTerm>")]
    #[serde(with = "crate::lotus_json")]
    pub beneficiary_term: BeneficiaryTerm,
    #[schemars(with = "LotusJson<Option<PendingBeneficiaryChange>>")]
    #[serde(with = "crate::lotus_json")]
    pub pending_beneficiary_term: Option<PendingBeneficiaryChange>,
}

#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub struct BeneficiaryTermLotusJson {
    /// The total amount the current beneficiary can withdraw. Monotonic, but reset when beneficiary changes.
    #[schemars(with = "LotusJson<TokenAmount>")]
    #[serde(with = "crate::lotus_json")]
    pub quota: TokenAmount,
    /// The amount of quota the current beneficiary has already withdrawn
    #[schemars(with = "LotusJson<TokenAmount>")]
    #[serde(with = "crate::lotus_json")]
    pub used_quota: TokenAmount,
    /// The epoch at which the beneficiary's rights expire and revert to the owner
    pub expiration: ChainEpoch,
}

#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub struct PendingBeneficiaryChangeLotusJson {
    #[schemars(with = "LotusJson<Address>")]
    #[serde(with = "crate::lotus_json")]
    pub new_beneficiary: Address,
    #[schemars(with = "LotusJson<TokenAmount>")]
    #[serde(with = "crate::lotus_json")]
    pub new_quota: TokenAmount,
    pub new_expiration: ChainEpoch,
    pub approved_by_beneficiary: bool,
    pub approved_by_nominee: bool,
}

#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub struct MinerPowerLotusJson {
    #[schemars(with = "LotusJson<Claim>")]
    #[serde(with = "crate::lotus_json")]
    miner_power: Claim,
    #[schemars(with = "LotusJson<Claim>")]
    #[serde(with = "crate::lotus_json")]
    total_power: Claim,
    has_min_power: bool,
}

#[derive(Serialize, Deserialize, PartialEq, Clone, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub struct ApiActorState {
    #[schemars(with = "LotusJson<TokenAmount>")]
    #[serde(with = "crate::lotus_json")]
    pub balance: TokenAmount,
    #[schemars(with = "LotusJson<Cid>")]
    #[serde(with = "crate::lotus_json")]
    pub code: Cid,
    #[schemars(with = "LotusJson<ApiState>")]
    #[serde(with = "crate::lotus_json")]
    pub state: ApiState,
}

lotus_json_with_self!(ApiActorState);

#[derive(Serialize, Deserialize, PartialEq, Clone, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub struct ApiState {
    #[schemars(with = "serde_json::Value")]
    #[serde(with = "crate::lotus_json")]
    pub builtin_actors: Ipld,
}

lotus_json_with_self!(ApiState);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
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

    /// Additional flags, see [`fil_actor_miner_state::v12::SectorOnChainInfoFlags`]
    pub flags: u32,

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

    /// Epoch at which this sector's power was most recently updated
    pub power_base_epoch: ChainEpoch,

    #[schemars(with = "LotusJson<TokenAmount>")]
    #[serde(with = "crate::lotus_json")]
    /// Expected twenty day projection of reward for sector computed at
    /// activation time
    pub expected_storage_pledge: TokenAmount,

    #[schemars(with = "LotusJson<TokenAmount>")]
    #[serde(with = "crate::lotus_json")]
    pub replaced_day_reward: TokenAmount,

    #[schemars(with = "LotusJson<Option<Cid>>")]
    #[serde(with = "crate::lotus_json", rename = "SectorKeyCID")]
    pub sector_key_cid: Option<Cid>,
}

lotus_json_with_self!(SectorOnChainInfo);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
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

#[derive(Clone, Serialize, Deserialize, PartialEq, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub struct ApiDeadline {
    #[schemars(with = "LotusJson<BitField>")]
    #[serde(with = "crate::lotus_json")]
    pub post_submissions: BitField,
    pub disputable_proof_count: u64,
}

lotus_json_with_self!(ApiDeadline);

#[derive(Clone, Serialize, Deserialize, PartialEq, JsonSchema)]
pub struct ApiDeadlineInfo(
    #[schemars(with = "String")]
    #[serde(with = "crate::lotus_json")]
    pub DeadlineInfo,
);
lotus_json_with_self!(ApiDeadlineInfo);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub struct CirculatingSupply {
    #[schemars(with = "LotusJson<TokenAmount>")]
    #[serde(with = "crate::lotus_json")]
    pub fil_vested: TokenAmount,
    #[schemars(with = "LotusJson<TokenAmount>")]
    #[serde(with = "crate::lotus_json")]
    pub fil_mined: TokenAmount,
    #[schemars(with = "LotusJson<TokenAmount>")]
    #[serde(with = "crate::lotus_json")]
    pub fil_burnt: TokenAmount,
    #[schemars(with = "LotusJson<TokenAmount>")]
    #[serde(with = "crate::lotus_json")]
    pub fil_locked: TokenAmount,
    #[schemars(with = "LotusJson<TokenAmount>")]
    #[serde(with = "crate::lotus_json")]
    pub fil_circulating: TokenAmount,
    #[schemars(with = "LotusJson<TokenAmount>")]
    #[serde(with = "crate::lotus_json")]
    pub fil_reserve_disbursed: TokenAmount,
}

lotus_json_with_self!(CirculatingSupply);

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub struct MinerSectors {
    live: u64,
    active: u64,
    faulty: u64,
}
lotus_json_with_self!(MinerSectors);

impl MinerSectors {
    pub fn new(live: u64, active: u64, faulty: u64) -> Self {
        Self {
            live,
            active,
            faulty,
        }
    }
}

#[derive(Clone, Serialize, Deserialize, PartialEq, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub struct MinerPartitions {
    #[schemars(with = "LotusJson<BitField>")]
    #[serde(with = "crate::lotus_json")]
    all_sectors: BitField,
    #[schemars(with = "LotusJson<BitField>")]
    #[serde(with = "crate::lotus_json")]
    faulty_sectors: BitField,
    #[schemars(with = "LotusJson<BitField>")]
    #[serde(with = "crate::lotus_json")]
    recovering_sectors: BitField,
    #[schemars(with = "LotusJson<BitField>")]
    #[serde(with = "crate::lotus_json")]
    live_sectors: BitField,
    #[schemars(with = "LotusJson<BitField>")]
    #[serde(with = "crate::lotus_json")]
    active_sectors: BitField,
}
lotus_json_with_self!(MinerPartitions);

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

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub struct Transaction {
    #[serde(rename = "ID")]
    pub id: i64,
    #[schemars(with = "LotusJson<Address>")]
    #[serde(with = "crate::lotus_json")]
    pub to: Address,
    #[schemars(with = "LotusJson<TokenAmount>")]
    #[serde(with = "crate::lotus_json")]
    pub value: TokenAmount,
    pub method: MethodNum,
    #[schemars(with = "LotusJson<RawBytes>")]
    #[serde(with = "crate::lotus_json")]
    pub params: RawBytes,
    #[schemars(with = "LotusJson<Vec<Address>>")]
    #[serde(with = "crate::lotus_json")]
    pub approved: Vec<Address>,
}

lotus_json_with_self!(Transaction);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub struct DealCollateralBounds {
    #[schemars(with = "LotusJson<TokenAmount>")]
    #[serde(with = "crate::lotus_json")]
    pub min: TokenAmount,
    #[schemars(with = "LotusJson<TokenAmount>")]
    #[serde(with = "crate::lotus_json")]
    pub max: TokenAmount,
}

lotus_json_with_self!(DealCollateralBounds);
