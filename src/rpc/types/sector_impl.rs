// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use fil_actor_miner_state::v13::SectorOnChainInfoFlags;

use super::*;

// `SectorOnChainInfo` conversions come in a few shapes. v8 and v12 are one-offs;
// the rest are generated per shape by `impl_sector_on_chain_info!` below.
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
            expected_day_reward: Some(info.expected_day_reward.into()),
            expected_storage_pledge: Some(info.expected_storage_pledge.into()),
            replaced_day_reward: None,
            sector_key_cid: info.sector_key_cid,
            power_base_epoch: info.activation,
            daily_fee: TokenAmount::default(),
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
            expected_day_reward: Some(info.expected_day_reward.into()),
            expected_storage_pledge: Some(info.expected_storage_pledge.into()),
            replaced_day_reward: Some(info.replaced_day_reward.into()),
            sector_key_cid: info.sector_key_cid,
            power_base_epoch: info.power_base_epoch,
            daily_fee: TokenAmount::default(),
        }
    }
}

macro_rules! impl_sector_on_chain_info {
    // v9-v11: `flags` derived from `simple_qa_power`, non-deprecated deal ids.
    (simple_qa: $($v:ident),+ $(,)?) => { $(
        impl From<fil_actor_miner_state::$v::SectorOnChainInfo> for SectorOnChainInfo {
            fn from(info: fil_actor_miner_state::$v::SectorOnChainInfo) -> Self {
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
                    expected_day_reward: Some(info.expected_day_reward.into()),
                    expected_storage_pledge: Some(info.expected_storage_pledge.into()),
                    replaced_day_reward: Some(info.replaced_day_reward.into()),
                    sector_key_cid: info.sector_key_cid,
                    power_base_epoch: info.activation,
                    daily_fee: TokenAmount::default(),
                }
            }
        }
    )+ };
    // v13-v15: `flags` bitfield, deprecated deal ids, non-optional reward fields.
    (flags_bits: $($v:ident),+ $(,)?) => { $(
        impl From<fil_actor_miner_state::$v::SectorOnChainInfo> for SectorOnChainInfo {
            fn from(info: fil_actor_miner_state::$v::SectorOnChainInfo) -> Self {
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
                    expected_day_reward: Some(info.expected_day_reward.into()),
                    expected_storage_pledge: Some(info.expected_storage_pledge.into()),
                    replaced_day_reward: Some(info.replaced_day_reward.into()),
                    sector_key_cid: info.sector_key_cid,
                    power_base_epoch: info.power_base_epoch,
                    daily_fee: TokenAmount::default(),
                }
            }
        }
    )+ };
    // v16+: optional reward fields and an on-chain `daily_fee`.
    (optional_rewards: $($v:ident),+ $(,)?) => { $(
        impl From<fil_actor_miner_state::$v::SectorOnChainInfo> for SectorOnChainInfo {
            fn from(info: fil_actor_miner_state::$v::SectorOnChainInfo) -> Self {
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
                    expected_day_reward: info.expected_day_reward.map(Into::into),
                    expected_storage_pledge: info.expected_storage_pledge.map(Into::into),
                    replaced_day_reward: info.replaced_day_reward.map(Into::into),
                    sector_key_cid: info.sector_key_cid,
                    power_base_epoch: info.power_base_epoch,
                    daily_fee: info.daily_fee.into(),
                }
            }
        }
    )+ };
}
impl_sector_on_chain_info!(simple_qa: v9, v10, v11);
impl_sector_on_chain_info!(flags_bits: v13, v14, v15);
impl_sector_on_chain_info!(optional_rewards: v16, v17, v18);

macro_rules! impl_sector_pre_commit_on_chain_info {
    ($($v:ident),+ $(,)?) => { $(
        impl From<fil_actor_miner_state::$v::SectorPreCommitOnChainInfo> for SectorPreCommitOnChainInfo {
            fn from(i: fil_actor_miner_state::$v::SectorPreCommitOnChainInfo) -> Self {
                Self {
                    info: i.info.into(),
                    pre_commit_deposit: i.pre_commit_deposit.into(),
                    pre_commit_epoch: i.pre_commit_epoch,
                }
            }
        }
    )+ };
}
impl_sector_pre_commit_on_chain_info!(v8, v9, v10, v11, v12, v13, v14, v15, v16, v17, v18);

// v8 predates the unsealed-CID field; every later version carries it.
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

macro_rules! impl_sector_pre_commit_info {
    ($($v:ident),+ $(,)?) => { $(
        impl From<fil_actor_miner_state::$v::SectorPreCommitInfo> for SectorPreCommitInfo {
            fn from(i: fil_actor_miner_state::$v::SectorPreCommitInfo) -> Self {
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
    )+ };
}
impl_sector_pre_commit_info!(v9, v10, v11, v12, v13, v14, v15, v16, v17, v18);

#[cfg(test)]
mod tests {
    use super::*;

    /// Smoke-checks that `From<fil_actor_miner_state::vN::$ty>` compiles and
    /// runs for every actor version, where the source and target share a name.
    macro_rules! assert_from_default {
        ($ty:ident, $($v:ident),+ $(,)?) => {
            $( let _: $ty = fil_actor_miner_state::$v::$ty::default().into(); )+
        };
    }

    #[test]
    fn sector_on_chain_info_conversions_from_all_versions() {
        assert_from_default!(
            SectorOnChainInfo,
            v8,
            v9,
            v10,
            v11,
            v12,
            v13,
            v14,
            v15,
            v16,
            v17,
            v18
        );
    }

    #[test]
    fn sector_on_chain_info_v9_simple_qa_power_sets_flag() {
        let info = fil_actor_miner_state::v9::SectorOnChainInfo {
            simple_qa_power: true,
            ..Default::default()
        };
        let converted: SectorOnChainInfo = info.into();
        assert_eq!(
            converted.flags,
            SectorOnChainInfoFlags::SIMPLE_QA_POWER.bits()
        );
    }

    #[test]
    fn sector_pre_commit_info_conversions_from_all_versions() {
        assert_from_default!(
            SectorPreCommitInfo,
            v8,
            v9,
            v10,
            v11,
            v12,
            v13,
            v14,
            v15,
            v16,
            v17,
            v18
        );
    }
}
