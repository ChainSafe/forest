// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;

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

impl From<fil_actor_miner_state::v8::SectorPreCommitOnChainInfo> for SectorPreCommitOnChainInfo {
    fn from(i: fil_actor_miner_state::v8::SectorPreCommitOnChainInfo) -> Self {
        Self {
            info: LotusJson(i.info.into()),
            pre_commit_deposit: LotusJson(i.pre_commit_deposit.into()),
            pre_commit_epoch: i.pre_commit_epoch,
        }
    }
}

impl From<fil_actor_miner_state::v9::SectorPreCommitOnChainInfo> for SectorPreCommitOnChainInfo {
    fn from(i: fil_actor_miner_state::v9::SectorPreCommitOnChainInfo) -> Self {
        Self {
            info: LotusJson(i.info.into()),
            pre_commit_deposit: LotusJson(i.pre_commit_deposit.into()),
            pre_commit_epoch: i.pre_commit_epoch,
        }
    }
}

impl From<fil_actor_miner_state::v10::SectorPreCommitOnChainInfo> for SectorPreCommitOnChainInfo {
    fn from(i: fil_actor_miner_state::v10::SectorPreCommitOnChainInfo) -> Self {
        Self {
            info: LotusJson(i.info.into()),
            pre_commit_deposit: LotusJson(i.pre_commit_deposit.into()),
            pre_commit_epoch: i.pre_commit_epoch,
        }
    }
}

impl From<fil_actor_miner_state::v11::SectorPreCommitOnChainInfo> for SectorPreCommitOnChainInfo {
    fn from(i: fil_actor_miner_state::v11::SectorPreCommitOnChainInfo) -> Self {
        Self {
            info: LotusJson(i.info.into()),
            pre_commit_deposit: LotusJson(i.pre_commit_deposit.into()),
            pre_commit_epoch: i.pre_commit_epoch,
        }
    }
}

impl From<fil_actor_miner_state::v12::SectorPreCommitOnChainInfo> for SectorPreCommitOnChainInfo {
    fn from(i: fil_actor_miner_state::v12::SectorPreCommitOnChainInfo) -> Self {
        Self {
            info: LotusJson(i.info.into()),
            pre_commit_deposit: LotusJson(i.pre_commit_deposit.into()),
            pre_commit_epoch: i.pre_commit_epoch,
        }
    }
}

impl From<fil_actor_miner_state::v13::SectorPreCommitOnChainInfo> for SectorPreCommitOnChainInfo {
    fn from(i: fil_actor_miner_state::v13::SectorPreCommitOnChainInfo) -> Self {
        Self {
            info: LotusJson(i.info.into()),
            pre_commit_deposit: LotusJson(i.pre_commit_deposit.into()),
            pre_commit_epoch: i.pre_commit_epoch,
        }
    }
}

impl From<fil_actor_miner_state::v8::SectorPreCommitInfo> for SectorPreCommitInfo {
    fn from(i: fil_actor_miner_state::v8::SectorPreCommitInfo) -> Self {
        Self {
            seal_proof: LotusJson(i.seal_proof.into()),
            sector_number: i.sector_number,
            sealed_cid: LotusJson(i.sealed_cid),
            seal_rand_epoch: i.seal_rand_epoch,
            deal_ids: Some(i.deal_ids),
            expiration: i.expiration,
            unsealed_cid: LotusJson(None),
        }
    }
}

impl From<fil_actor_miner_state::v9::SectorPreCommitInfo> for SectorPreCommitInfo {
    fn from(i: fil_actor_miner_state::v9::SectorPreCommitInfo) -> Self {
        Self {
            seal_proof: LotusJson(i.seal_proof.into()),
            sector_number: i.sector_number,
            sealed_cid: LotusJson(i.sealed_cid),
            seal_rand_epoch: i.seal_rand_epoch,
            deal_ids: Some(i.deal_ids),
            expiration: i.expiration,
            unsealed_cid: LotusJson(i.unsealed_cid.0),
        }
    }
}

impl From<fil_actor_miner_state::v10::SectorPreCommitInfo> for SectorPreCommitInfo {
    fn from(i: fil_actor_miner_state::v10::SectorPreCommitInfo) -> Self {
        Self {
            seal_proof: LotusJson(i.seal_proof.into()),
            sector_number: i.sector_number,
            sealed_cid: LotusJson(i.sealed_cid),
            seal_rand_epoch: i.seal_rand_epoch,
            deal_ids: Some(i.deal_ids),
            expiration: i.expiration,
            unsealed_cid: LotusJson(i.unsealed_cid.0),
        }
    }
}

impl From<fil_actor_miner_state::v11::SectorPreCommitInfo> for SectorPreCommitInfo {
    fn from(i: fil_actor_miner_state::v11::SectorPreCommitInfo) -> Self {
        Self {
            seal_proof: LotusJson(i.seal_proof.into()),
            sector_number: i.sector_number,
            sealed_cid: LotusJson(i.sealed_cid),
            seal_rand_epoch: i.seal_rand_epoch,
            deal_ids: Some(i.deal_ids),
            expiration: i.expiration,
            unsealed_cid: LotusJson(i.unsealed_cid.0),
        }
    }
}

impl From<fil_actor_miner_state::v12::SectorPreCommitInfo> for SectorPreCommitInfo {
    fn from(i: fil_actor_miner_state::v12::SectorPreCommitInfo) -> Self {
        Self {
            seal_proof: LotusJson(i.seal_proof.into()),
            sector_number: i.sector_number,
            sealed_cid: LotusJson(i.sealed_cid),
            seal_rand_epoch: i.seal_rand_epoch,
            deal_ids: Some(i.deal_ids),
            expiration: i.expiration,
            unsealed_cid: LotusJson(i.unsealed_cid.0),
        }
    }
}

impl From<fil_actor_miner_state::v13::SectorPreCommitInfo> for SectorPreCommitInfo {
    fn from(i: fil_actor_miner_state::v13::SectorPreCommitInfo) -> Self {
        Self {
            seal_proof: LotusJson(i.seal_proof.into()),
            sector_number: i.sector_number,
            sealed_cid: LotusJson(i.sealed_cid),
            seal_rand_epoch: i.seal_rand_epoch,
            deal_ids: Some(i.deal_ids),
            expiration: i.expiration,
            unsealed_cid: LotusJson(i.unsealed_cid.0),
        }
    }
}
