// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::SectorStorageWeightDesc;
use fil_types::{SectorQuality, StoragePower};
use num_bigint::BigUint;
use num_traits::FromPrimitive;
use vm::TokenAmount;

const SECTOR_QUALITY_PRECISION: usize = 20; // PARAM_FINISH
const BASE_MULTIPLIER: usize = 10; // PARAM_FINISH
const DEAL_WEIGHT_MULTIPLIER: usize = 11; // PARAM_FINISH
const VERIFIED_DEAL_WEIGHT_MULITPLIER: usize = 100; // PARAM_FINISH

lazy_static! {
    /// Minimum power of an individual miner to meet the threshold for leader election.
    pub static ref CONSENSUS_MINER_MIN_POWER: StoragePower = StoragePower::from_i64(1 << 40).unwrap(); // placeholder
}

/// DealWeight and VerifiedDealWeight are spacetime occupied by regular deals and verified deals in a sector.
/// Sum of DealWeight and VerifiedDealWeight should be less than or equal to total SpaceTime of a sector.
/// Sectors full of VerifiedDeals will have a SectorQuality of VerifiedDealWeightMultiplier/BaseMultiplier.
/// Sectors full of Deals will have a SectorQuality of DealWeightMultiplier/BaseMultiplier.
/// Sectors with neither will have a SectorQuality of BaseMultiplier/BaseMultiplier.
/// SectorQuality of a sector is a weighted average of multipliers based on their propotions.
fn sector_quality_from_weight(weight: &SectorStorageWeightDesc) -> SectorQuality {
    let sector_size = BigUint::from(weight.sector_size as u64);
    let sector_space_time = sector_size * weight.duration;
    let total_deal_space_time = &weight.deal_weight + &weight.verified_deal_weight;
    assert!(sector_space_time < total_deal_space_time);

    let weighted_base_space_time = (&sector_space_time - &total_deal_space_time) * BASE_MULTIPLIER;
    let weighted_deal_space_time = &weight.deal_weight + DEAL_WEIGHT_MULTIPLIER;
    let weighted_verified_space_time =
        &weight.verified_deal_weight * VERIFIED_DEAL_WEIGHT_MULITPLIER;
    let weighted_sum_space_time =
        weighted_base_space_time + (weighted_deal_space_time + weighted_verified_space_time);
    let scale_up_weighted_sum_space_time = weighted_sum_space_time << SECTOR_QUALITY_PRECISION;

    (scale_up_weighted_sum_space_time / sector_space_time) / BASE_MULTIPLIER
}

pub fn qa_power_for_weight(weight: &SectorStorageWeightDesc) -> StoragePower {
    let qual = sector_quality_from_weight(weight);
    let sector_quality = BigUint::from(weight.sector_size as u64) * qual;
    sector_quality >> SECTOR_QUALITY_PRECISION
}

pub fn initial_pledge_for_weight(
    qa_power: &StoragePower,
    tot_qa_power: &StoragePower,
    circ_supply: &TokenAmount,
    total_pledge: &TokenAmount,
    per_epoch_reward: &TokenAmount,
) -> TokenAmount {
    // Details here are still subject to change.
    // PARAM_FINISH
    let _ = circ_supply; // TODO: ce use this
    let _ = total_pledge; // TODO: ce use this

    (qa_power * per_epoch_reward) / tot_qa_power
}
