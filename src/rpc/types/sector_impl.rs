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
