// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use cid::Cid;
use forest_fil_types::deadlines::DeadlineInfo;
use forest_ipld_blockstore::{BlockStore, BlockStoreExt};
use forest_json::bigint::json;
use forest_utils::json::go_vec_visitor;
use fvm::state_tree::ActorState;
use fvm_ipld_bitfield::BitField;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::BytesDe;
use fvm_shared::bigint::BigInt;
use fvm_shared::clock::ChainEpoch;
use fvm_shared::deal::DealID;
use fvm_shared::sector::{RegisteredPoStProof, RegisteredSealProof, SectorNumber, SectorSize};
use fvm_shared::{address::Address, econ::TokenAmount};
use libp2p::PeerId;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;

use anyhow::Context;

use crate::power::Claim;
/// Miner actor method.
pub type Method = fil_actor_miner_v8::Method;

pub fn is_v8_miner_cid(cid: &Cid) -> bool {
    let known_cids = vec![
        // calibnet
        Cid::try_from("bafk2bzacea6rabflc7kpwr6y4lzcqsnuahr4zblyq3rhzrrsfceeiw2lufrb4").unwrap(),
        // mainnet
        Cid::try_from("bafk2bzacecgnynvd3tene3bvqoknuspit56canij5bpra6wl4mrq2mxxwriyu").unwrap(),
        // devnet
        Cid::try_from("bafk2bzacebze3elvppssc6v5457ukszzy6ndrg6xgaojfsqfbbtg3xfwo4rbs").unwrap(),
    ];
    known_cids.contains(cid)
}

/// Miner actor state.
#[derive(Serialize)]
#[serde(untagged)]
pub enum State {
    // V7(fil_actor_miner_v7::State),
    V8(fil_actor_miner_v8::State),
}

impl State {
    pub fn load<BS>(store: &BS, actor: &ActorState) -> anyhow::Result<State>
    where
        BS: Blockstore,
    {
        if is_v8_miner_cid(&actor.code) {
            return store
                .get_obj(&actor.state)?
                .map(State::V8)
                .context("Actor state doesn't exist in store");
        }
        Err(anyhow::anyhow!("Unknown miner actor code {}", actor.code))
    }

    pub fn info<BS: Blockstore>(&self, store: &BS) -> anyhow::Result<MinerInfo> {
        match self {
            State::V8(st) => {
                let info = st.get_info(store)?;

                // Deserialize into peer id if valid, `None` if not.
                let peer_id = PeerId::from_bytes(&info.peer_id).ok();

                Ok(MinerInfo {
                    owner: info.owner,
                    worker: info.worker,
                    control_addresses: info.control_addresses,
                    new_worker: info.pending_worker_key.as_ref().map(|k| k.new_worker),
                    worker_change_epoch: info
                        .pending_worker_key
                        .map(|k| k.effective_at)
                        .unwrap_or(-1),
                    peer_id,
                    multiaddrs: info.multi_address,
                    window_post_proof_type: info.window_post_proof_type,
                    sector_size: info.sector_size,
                    window_post_partition_sectors: info.window_post_partition_sectors,
                    consensus_fault_elapsed: info.consensus_fault_elapsed,
                })
            }
        }
    }

    /// Loads deadlines for a miner's state
    pub fn for_each_deadline<BS: Blockstore>(
        &self,
        store: &BS,
        mut f: impl FnMut(u64, Deadline) -> Result<(), anyhow::Error>,
    ) -> anyhow::Result<()> {
        match self {
            State::V8(st) => {
                st.load_deadlines(&store)?
                    .for_each(&Default::default(), &store, |idx, dl| {
                        f(idx as u64, Deadline::V8(dl))
                    })
            }
        }
    }

    /// Loads deadline at index for a miner's state
    pub fn load_deadline<BS: Blockstore>(
        &self,
        _store: &BS,
        _idx: u64,
    ) -> anyhow::Result<Deadline> {
        unimplemented!()
    }

    /// Loads sectors corresponding to the bitfield. If no bitfield is passed in, return all.
    pub fn load_sectors<BS: Blockstore>(
        &self,
        store: &BS,
        sectors: Option<&BitField>,
    ) -> anyhow::Result<Vec<SectorOnChainInfo>> {
        match self {
            State::V8(st) => {
                if let Some(sectors) = sectors {
                    Ok(st
                        .load_sector_infos(&store, sectors)?
                        .into_iter()
                        .map(From::from)
                        .collect())
                } else {
                    let sectors = fil_actor_miner_v8::Sectors::load(&store, &st.sectors)?;
                    let mut infos = Vec::with_capacity(sectors.amt.count() as usize);
                    sectors.amt.for_each(|_, info| {
                        infos.push(SectorOnChainInfo::from(info.clone()));
                        Ok(())
                    })?;
                    Ok(infos)
                }
            }
        }
    }

    /// Gets pre-committed on chain info
    pub fn get_precommitted_sector<BS: Blockstore>(
        &self,
        _store: &BS,
        _sector_num: SectorNumber,
    ) -> anyhow::Result<Option<SectorPreCommitOnChainInfo>> {
        unimplemented!()
    }

    /// Loads a specific sector number
    pub fn get_sector<BS: Blockstore>(
        &self,
        _store: &BS,
        _sector_num: u64,
    ) -> anyhow::Result<Option<SectorOnChainInfo>> {
        unimplemented!()
    }

    /// Loads deadline at index for a miner's state
    pub fn deadline_info(&self, _epoch: ChainEpoch) -> DeadlineInfo {
        match self {
            State::V8(_st) => todo!(),
        }
    }

    /// Gets fee debt of miner state
    pub fn fee_debt(&self) -> TokenAmount {
        match self {
            State::V8(st) => st.fee_debt.clone(),
        }
    }
}

/// Static information about miner
#[derive(Debug, PartialEq, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct MinerInfo {
    #[serde(with = "forest_json::address::json")]
    pub owner: Address,
    #[serde(with = "forest_json::address::json")]
    pub worker: Address,
    #[serde(with = "forest_json::address::json::opt")]
    pub new_worker: Option<Address>,
    #[serde(with = "forest_json::address::json::vec")]
    pub control_addresses: Vec<Address>, // Must all be ID addresses.
    pub worker_change_epoch: ChainEpoch,
    #[serde(with = "peer_id_json")]
    pub peer_id: Option<PeerId>,
    pub multiaddrs: Vec<BytesDe>,
    pub window_post_proof_type: RegisteredPoStProof,
    pub sector_size: SectorSize,
    pub window_post_partition_sectors: u64,
    pub consensus_fault_elapsed: ChainEpoch,
}

impl MinerInfo {
    pub fn worker(&self) -> Address {
        self.worker
    }

    pub fn sector_size(&self) -> SectorSize {
        self.sector_size
    }
}

#[derive(Serialize, Deserialize)]
pub struct MinerPower {
    pub miner_power: Claim,
    pub total_power: Claim,
    pub has_min_power: bool,
}

/// Deadline holds the state for all sectors due at a specific deadline.
pub enum Deadline {
    V8(fil_actor_miner_v8::Deadline),
}

impl Deadline {
    /// For each partition of the deadline
    pub fn for_each<BS: BlockStore>(
        &self,
        store: &BS,
        mut f: impl FnMut(u64, Partition) -> Result<(), anyhow::Error>,
    ) -> anyhow::Result<()> {
        match self {
            Deadline::V8(dl) => dl.for_each(&store, |idx, part| {
                f(idx as u64, Partition::V8(Cow::Borrowed(part)))
            }),
        }
    }

    pub fn disputable_proof_count<BS: BlockStore>(&self, store: &BS) -> anyhow::Result<usize> {
        Ok(match self {
            Deadline::V8(dl) => dl
                .optimistic_proofs_snapshot_amt(&store)?
                .count()
                .try_into()
                .unwrap(),
        })
    }

    pub fn partitions_posted(&self) -> &BitField {
        todo!()
    }
}

#[allow(clippy::large_enum_variant)]
pub enum Partition<'a> {
    // V7(Cow<'a, fil_actor_miner_v7::Partition>),
    V8(Cow<'a, fil_actor_miner_v8::Partition>),
}

impl Partition<'_> {
    pub fn all_sectors(&self) -> &BitField {
        todo!()
    }
    pub fn faulty_sectors(&self) -> &BitField {
        todo!()
    }
    pub fn recovering_sectors(&self) -> &BitField {
        todo!()
    }
    pub fn live_sectors(&self) -> BitField {
        match self {
            Partition::V8(dl) => dl.live_sectors(),
        }
    }
    pub fn active_sectors(&self) -> BitField {
        match self {
            Partition::V8(dl) => dl.active_sectors(),
        }
    }
}

mod peer_id_json {
    use super::*;
    use serde::Serializer;

    pub fn serialize<S>(m: &Option<PeerId>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        m.as_ref().map(|pid| pid.to_string()).serialize(serializer)
    }
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct SectorOnChainInfo {
    pub sector_number: SectorNumber,
    /// The seal proof type implies the PoSt proofs
    pub seal_proof: RegisteredSealProof,
    /// `CommR`
    #[serde(with = "forest_json::cid")]
    pub sealed_cid: Cid,
    pub deal_ids: Vec<DealID>,
    /// Epoch during which the sector proof was accepted
    pub activation: ChainEpoch,
    /// Epoch during which the sector expires
    pub expiration: ChainEpoch,
    /// Integral of active deals over sector lifetime
    #[serde(with = "json")]
    pub deal_weight: BigInt,
    /// Integral of active verified deals over sector lifetime
    #[serde(with = "json")]
    pub verified_deal_weight: BigInt,
    /// Pledge collected to commit this sector
    #[serde(with = "json")]
    pub initial_pledge: TokenAmount,
    /// Expected one day projection of reward for sector computed at activation time
    #[serde(with = "json")]
    pub expected_day_reward: TokenAmount,
    /// Expected twenty day projection of reward for sector computed at activation time
    #[serde(with = "json")]
    pub expected_storage_pledge: TokenAmount,
}

impl From<fil_actor_miner_v8::SectorOnChainInfo> for SectorOnChainInfo {
    fn from(info: fil_actor_miner_v8::SectorOnChainInfo) -> Self {
        Self {
            sector_number: info.sector_number,
            seal_proof: info.seal_proof,
            sealed_cid: info.sealed_cid,
            deal_ids: info.deal_ids,
            activation: info.activation,
            expiration: info.expiration,
            deal_weight: info.deal_weight,
            verified_deal_weight: info.verified_deal_weight,
            initial_pledge: info.initial_pledge,
            expected_day_reward: info.expected_day_reward,
            expected_storage_pledge: info.expected_storage_pledge,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct SectorPreCommitOnChainInfo {
    pub info: SectorPreCommitInfo,
    #[serde(with = "json")]
    pub pre_commit_deposit: TokenAmount,
    pub pre_commit_epoch: ChainEpoch,
    /// Integral of active deals over sector lifetime, 0 if `CommittedCapacity` sector
    #[serde(with = "json")]
    pub deal_weight: BigInt,
    /// Integral of active verified deals over sector lifetime
    #[serde(with = "json")]
    pub verified_deal_weight: BigInt,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct SectorPreCommitInfo {
    pub seal_proof: RegisteredSealProof,
    pub sector_number: SectorNumber,
    /// `CommR`
    #[serde(with = "forest_json::cid", rename = "SealedCID")]
    pub sealed_cid: Cid,
    pub seal_rand_epoch: ChainEpoch,
    #[serde(with = "go_vec_visitor", rename = "DealIDs")]
    pub deal_ids: Vec<DealID>,
    pub expiration: ChainEpoch,
    /// Whether to replace a "committed capacity" no-deal sector (requires non-empty `DealIDs`)
    pub replace_capacity: bool,
    /// The committed capacity sector to replace, and its deadline/partition location
    pub replace_sector_deadline: u64,
    pub replace_sector_partition: u64,
    pub replace_sector_number: SectorNumber,
}
