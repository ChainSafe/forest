// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use fil_actor_miner_state::v13::SectorOnChainInfoFlags;

use super::*;

impl From<fil_actor_miner_state::v8::SectorOnChainInfo> for SectorOnChainInfo {
    fn from(info: fil_actor_miner_state::v8::SectorOnChainInfo) -> Self {
        Self {
            sector_number: info.sector_number,
            seal_proof: info.seal_proof.into(),
            sealed_cid: info.sealed_cid,
            deal_ids: info.deal_ids,
            activation: info.activation,
            expiration: info.expiration,
            flags: Default::default(),
            deal_weight: info.deal_weight,
            verified_deal_weight: info.verified_deal_weight,
            initial_pledge: info.initial_pledge.into(),
            expected_day_reward: info.expected_day_reward.into(),
            expected_storage_pledge: info.expected_storage_pledge.into(),
            replaced_day_reward: TokenAmount::default(),
            sector_key_cid: info.sector_key_cid,
            power_base_epoch: info.activation,
            daily_fee: Default::default(),
        }
    }
}

impl From<fil_actor_miner_state::v9::SectorOnChainInfo> for SectorOnChainInfo {
    fn from(info: fil_actor_miner_state::v9::SectorOnChainInfo) -> Self {
        Self {
            sector_number: info.sector_number,
            seal_proof: info.seal_proof.into(),
            sealed_cid: info.sealed_cid,
            deal_ids: info.deal_ids,
            activation: info.activation,
            expiration: info.expiration,
            flags: if info.simple_qa_power {
                SectorOnChainInfoFlags::SIMPLE_QA_POWER.bits()
            } else {
                Default::default()
            },
            deal_weight: info.deal_weight,
            verified_deal_weight: info.verified_deal_weight,
            initial_pledge: info.initial_pledge.into(),
            expected_day_reward: info.expected_day_reward.into(),
            expected_storage_pledge: info.expected_storage_pledge.into(),
            replaced_day_reward: info.replaced_day_reward.into(),
            sector_key_cid: info.sector_key_cid,
            power_base_epoch: info.activation,
            daily_fee: Default::default(),
        }
    }
}

impl From<fil_actor_miner_state::v10::SectorOnChainInfo> for SectorOnChainInfo {
    fn from(info: fil_actor_miner_state::v10::SectorOnChainInfo) -> Self {
        Self {
            sector_number: info.sector_number,
            seal_proof: info.seal_proof.into(),
            sealed_cid: info.sealed_cid,
            deal_ids: info.deal_ids,
            activation: info.activation,
            expiration: info.expiration,
            flags: if info.simple_qa_power {
                SectorOnChainInfoFlags::SIMPLE_QA_POWER.bits()
            } else {
                Default::default()
            },
            deal_weight: info.deal_weight,
            verified_deal_weight: info.verified_deal_weight,
            initial_pledge: info.initial_pledge.into(),
            expected_day_reward: info.expected_day_reward.into(),
            expected_storage_pledge: info.expected_storage_pledge.into(),
            replaced_day_reward: info.replaced_day_reward.into(),
            sector_key_cid: info.sector_key_cid,
            power_base_epoch: info.activation,
            daily_fee: Default::default(),
        }
    }
}

impl From<fil_actor_miner_state::v11::SectorOnChainInfo> for SectorOnChainInfo {
    fn from(info: fil_actor_miner_state::v11::SectorOnChainInfo) -> Self {
        Self {
            sector_number: info.sector_number,
            seal_proof: info.seal_proof.into(),
            sealed_cid: info.sealed_cid,
            deal_ids: info.deal_ids,
            activation: info.activation,
            expiration: info.expiration,
            flags: if info.simple_qa_power {
                SectorOnChainInfoFlags::SIMPLE_QA_POWER.bits()
            } else {
                Default::default()
            },
            deal_weight: info.deal_weight,
            verified_deal_weight: info.verified_deal_weight,
            initial_pledge: info.initial_pledge.into(),
            expected_day_reward: info.expected_day_reward.into(),
            expected_storage_pledge: info.expected_storage_pledge.into(),
            replaced_day_reward: info.replaced_day_reward.into(),
            sector_key_cid: info.sector_key_cid,
            power_base_epoch: info.activation,
            daily_fee: Default::default(),
        }
    }
}

impl From<fil_actor_miner_state::v12::SectorOnChainInfo> for SectorOnChainInfo {
    fn from(info: fil_actor_miner_state::v12::SectorOnChainInfo) -> Self {
        Self {
            sector_number: info.sector_number,
            seal_proof: info.seal_proof.into(),
            sealed_cid: info.sealed_cid,
            deal_ids: info.deal_ids,
            activation: info.activation,
            expiration: info.expiration,
            flags: info.flags.bits(),
            deal_weight: info.deal_weight,
            verified_deal_weight: info.verified_deal_weight,
            initial_pledge: info.initial_pledge.into(),
            expected_day_reward: info.expected_day_reward.into(),
            expected_storage_pledge: info.expected_storage_pledge.into(),
            replaced_day_reward: info.replaced_day_reward.into(),
            sector_key_cid: info.sector_key_cid,
            power_base_epoch: info.power_base_epoch,
            daily_fee: Default::default(),
        }
    }
}

impl From<fil_actor_miner_state::v13::SectorOnChainInfo> for SectorOnChainInfo {
    fn from(info: fil_actor_miner_state::v13::SectorOnChainInfo) -> Self {
        Self {
            sector_number: info.sector_number,
            seal_proof: info.seal_proof.into(),
            sealed_cid: info.sealed_cid,
            deal_ids: info.deprecated_deal_ids,
            activation: info.activation,
            expiration: info.expiration,
            flags: info.flags.bits(),
            deal_weight: info.deal_weight,
            verified_deal_weight: info.verified_deal_weight,
            initial_pledge: info.initial_pledge.into(),
            expected_day_reward: info.expected_day_reward.into(),
            expected_storage_pledge: info.expected_storage_pledge.into(),
            replaced_day_reward: info.replaced_day_reward.into(),
            sector_key_cid: info.sector_key_cid,
            power_base_epoch: info.power_base_epoch,
            daily_fee: Default::default(),
        }
    }
}

impl From<fil_actor_miner_state::v14::SectorOnChainInfo> for SectorOnChainInfo {
    fn from(info: fil_actor_miner_state::v14::SectorOnChainInfo) -> Self {
        Self {
            sector_number: info.sector_number,
            seal_proof: info.seal_proof.into(),
            sealed_cid: info.sealed_cid,
            deal_ids: info.deprecated_deal_ids,
            activation: info.activation,
            expiration: info.expiration,
            flags: info.flags.bits(),
            deal_weight: info.deal_weight,
            verified_deal_weight: info.verified_deal_weight,
            initial_pledge: info.initial_pledge.into(),
            expected_day_reward: info.expected_day_reward.into(),
            expected_storage_pledge: info.expected_storage_pledge.into(),
            replaced_day_reward: info.replaced_day_reward.into(),
            sector_key_cid: info.sector_key_cid,
            power_base_epoch: info.power_base_epoch,
            daily_fee: Default::default(),
        }
    }
}

impl From<fil_actor_miner_state::v15::SectorOnChainInfo> for SectorOnChainInfo {
    fn from(info: fil_actor_miner_state::v15::SectorOnChainInfo) -> Self {
        Self {
            sector_number: info.sector_number,
            seal_proof: info.seal_proof.into(),
            sealed_cid: info.sealed_cid,
            deal_ids: info.deprecated_deal_ids,
            activation: info.activation,
            expiration: info.expiration,
            flags: info.flags.bits(),
            deal_weight: info.deal_weight,
            verified_deal_weight: info.verified_deal_weight,
            initial_pledge: info.initial_pledge.into(),
            expected_day_reward: info.expected_day_reward.into(),
            expected_storage_pledge: info.expected_storage_pledge.into(),
            replaced_day_reward: info.replaced_day_reward.into(),
            sector_key_cid: info.sector_key_cid,
            power_base_epoch: info.power_base_epoch,
            daily_fee: Default::default(),
        }
    }
}

impl From<fil_actor_miner_state::v16::SectorOnChainInfo> for SectorOnChainInfo {
    fn from(info: fil_actor_miner_state::v16::SectorOnChainInfo) -> Self {
        Self {
            sector_number: info.sector_number,
            seal_proof: info.seal_proof.into(),
            sealed_cid: info.sealed_cid,
            deal_ids: info.deprecated_deal_ids,
            activation: info.activation,
            expiration: info.expiration,
            flags: info.flags.bits(),
            deal_weight: info.deal_weight,
            verified_deal_weight: info.verified_deal_weight,
            initial_pledge: info.initial_pledge.into(),
            expected_day_reward: info
                .expected_day_reward
                .unwrap_or(TokenAmount::default().into())
                .into(),
            expected_storage_pledge: info
                .expected_storage_pledge
                .unwrap_or(TokenAmount::default().into())
                .into(),
            replaced_day_reward: info
                .replaced_day_reward
                .unwrap_or(TokenAmount::default().into())
                .into(),
            sector_key_cid: info.sector_key_cid,
            power_base_epoch: info.power_base_epoch,
            daily_fee: info.daily_fee.into(),
        }
    }
}

impl From<fil_actor_miner_state::v8::SectorPreCommitOnChainInfo> for SectorPreCommitOnChainInfo {
    fn from(i: fil_actor_miner_state::v8::SectorPreCommitOnChainInfo) -> Self {
        Self {
            info: i.info.into(),
            pre_commit_deposit: i.pre_commit_deposit.into(),
            pre_commit_epoch: i.pre_commit_epoch,
        }
    }
}

impl From<fil_actor_miner_state::v9::SectorPreCommitOnChainInfo> for SectorPreCommitOnChainInfo {
    fn from(i: fil_actor_miner_state::v9::SectorPreCommitOnChainInfo) -> Self {
        Self {
            info: i.info.into(),
            pre_commit_deposit: i.pre_commit_deposit.into(),
            pre_commit_epoch: i.pre_commit_epoch,
        }
    }
}

impl From<fil_actor_miner_state::v10::SectorPreCommitOnChainInfo> for SectorPreCommitOnChainInfo {
    fn from(i: fil_actor_miner_state::v10::SectorPreCommitOnChainInfo) -> Self {
        Self {
            info: i.info.into(),
            pre_commit_deposit: i.pre_commit_deposit.into(),
            pre_commit_epoch: i.pre_commit_epoch,
        }
    }
}

impl From<fil_actor_miner_state::v11::SectorPreCommitOnChainInfo> for SectorPreCommitOnChainInfo {
    fn from(i: fil_actor_miner_state::v11::SectorPreCommitOnChainInfo) -> Self {
        Self {
            info: i.info.into(),
            pre_commit_deposit: i.pre_commit_deposit.into(),
            pre_commit_epoch: i.pre_commit_epoch,
        }
    }
}

impl From<fil_actor_miner_state::v12::SectorPreCommitOnChainInfo> for SectorPreCommitOnChainInfo {
    fn from(i: fil_actor_miner_state::v12::SectorPreCommitOnChainInfo) -> Self {
        Self {
            info: i.info.into(),
            pre_commit_deposit: i.pre_commit_deposit.into(),
            pre_commit_epoch: i.pre_commit_epoch,
        }
    }
}

impl From<fil_actor_miner_state::v13::SectorPreCommitOnChainInfo> for SectorPreCommitOnChainInfo {
    fn from(i: fil_actor_miner_state::v13::SectorPreCommitOnChainInfo) -> Self {
        Self {
            info: i.info.into(),
            pre_commit_deposit: i.pre_commit_deposit.into(),
            pre_commit_epoch: i.pre_commit_epoch,
        }
    }
}

impl From<fil_actor_miner_state::v14::SectorPreCommitOnChainInfo> for SectorPreCommitOnChainInfo {
    fn from(i: fil_actor_miner_state::v14::SectorPreCommitOnChainInfo) -> Self {
        Self {
            info: i.info.into(),
            pre_commit_deposit: i.pre_commit_deposit.into(),
            pre_commit_epoch: i.pre_commit_epoch,
        }
    }
}

impl From<fil_actor_miner_state::v15::SectorPreCommitOnChainInfo> for SectorPreCommitOnChainInfo {
    fn from(i: fil_actor_miner_state::v15::SectorPreCommitOnChainInfo) -> Self {
        Self {
            info: i.info.into(),
            pre_commit_deposit: i.pre_commit_deposit.into(),
            pre_commit_epoch: i.pre_commit_epoch,
        }
    }
}

impl From<fil_actor_miner_state::v16::SectorPreCommitOnChainInfo> for SectorPreCommitOnChainInfo {
    fn from(i: fil_actor_miner_state::v16::SectorPreCommitOnChainInfo) -> Self {
        Self {
            info: i.info.into(),
            pre_commit_deposit: i.pre_commit_deposit.into(),
            pre_commit_epoch: i.pre_commit_epoch,
        }
    }
}

impl From<fil_actor_miner_state::v8::SectorPreCommitInfo> for SectorPreCommitInfo {
    fn from(i: fil_actor_miner_state::v8::SectorPreCommitInfo) -> Self {
        Self {
            seal_proof: i.seal_proof.into(),
            sector_number: i.sector_number,
            sealed_cid: i.sealed_cid,
            seal_rand_epoch: i.seal_rand_epoch,
            deal_ids: i.deal_ids,
            expiration: i.expiration,
            unsealed_cid: None,
        }
    }
}

impl From<fil_actor_miner_state::v9::SectorPreCommitInfo> for SectorPreCommitInfo {
    fn from(i: fil_actor_miner_state::v9::SectorPreCommitInfo) -> Self {
        Self {
            seal_proof: i.seal_proof.into(),
            sector_number: i.sector_number,
            sealed_cid: i.sealed_cid,
            seal_rand_epoch: i.seal_rand_epoch,
            deal_ids: i.deal_ids,
            expiration: i.expiration,
            unsealed_cid: i.unsealed_cid.0,
        }
    }
}

impl From<fil_actor_miner_state::v10::SectorPreCommitInfo> for SectorPreCommitInfo {
    fn from(i: fil_actor_miner_state::v10::SectorPreCommitInfo) -> Self {
        Self {
            seal_proof: i.seal_proof.into(),
            sector_number: i.sector_number,
            sealed_cid: i.sealed_cid,
            seal_rand_epoch: i.seal_rand_epoch,
            deal_ids: i.deal_ids,
            expiration: i.expiration,
            unsealed_cid: i.unsealed_cid.0,
        }
    }
}

impl From<fil_actor_miner_state::v11::SectorPreCommitInfo> for SectorPreCommitInfo {
    fn from(i: fil_actor_miner_state::v11::SectorPreCommitInfo) -> Self {
        Self {
            seal_proof: i.seal_proof.into(),
            sector_number: i.sector_number,
            sealed_cid: i.sealed_cid,
            seal_rand_epoch: i.seal_rand_epoch,
            deal_ids: i.deal_ids,
            expiration: i.expiration,
            unsealed_cid: i.unsealed_cid.0,
        }
    }
}

impl From<fil_actor_miner_state::v12::SectorPreCommitInfo> for SectorPreCommitInfo {
    fn from(i: fil_actor_miner_state::v12::SectorPreCommitInfo) -> Self {
        Self {
            seal_proof: i.seal_proof.into(),
            sector_number: i.sector_number,
            sealed_cid: i.sealed_cid,
            seal_rand_epoch: i.seal_rand_epoch,
            deal_ids: i.deal_ids,
            expiration: i.expiration,
            unsealed_cid: i.unsealed_cid.0,
        }
    }
}

impl From<fil_actor_miner_state::v13::SectorPreCommitInfo> for SectorPreCommitInfo {
    fn from(i: fil_actor_miner_state::v13::SectorPreCommitInfo) -> Self {
        Self {
            seal_proof: i.seal_proof.into(),
            sector_number: i.sector_number,
            sealed_cid: i.sealed_cid,
            seal_rand_epoch: i.seal_rand_epoch,
            deal_ids: i.deal_ids,
            expiration: i.expiration,
            unsealed_cid: i.unsealed_cid.0,
        }
    }
}

impl From<fil_actor_miner_state::v14::SectorPreCommitInfo> for SectorPreCommitInfo {
    fn from(i: fil_actor_miner_state::v14::SectorPreCommitInfo) -> Self {
        Self {
            seal_proof: i.seal_proof.into(),
            sector_number: i.sector_number,
            sealed_cid: i.sealed_cid,
            seal_rand_epoch: i.seal_rand_epoch,
            deal_ids: i.deal_ids,
            expiration: i.expiration,
            unsealed_cid: i.unsealed_cid.0,
        }
    }
}

impl From<fil_actor_miner_state::v15::SectorPreCommitInfo> for SectorPreCommitInfo {
    fn from(i: fil_actor_miner_state::v15::SectorPreCommitInfo) -> Self {
        Self {
            seal_proof: i.seal_proof.into(),
            sector_number: i.sector_number,
            sealed_cid: i.sealed_cid,
            seal_rand_epoch: i.seal_rand_epoch,
            deal_ids: i.deal_ids,
            expiration: i.expiration,
            unsealed_cid: i.unsealed_cid.0,
        }
    }
}

impl From<fil_actor_miner_state::v16::SectorPreCommitInfo> for SectorPreCommitInfo {
    fn from(i: fil_actor_miner_state::v16::SectorPreCommitInfo) -> Self {
        Self {
            seal_proof: i.seal_proof.into(),
            sector_number: i.sector_number,
            sealed_cid: i.sealed_cid,
            seal_rand_epoch: i.seal_rand_epoch,
            deal_ids: i.deal_ids,
            expiration: i.expiration,
            unsealed_cid: i.unsealed_cid.0,
        }
    }
}
