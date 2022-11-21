// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::cmp::max;

use fil_actors_runtime_v8::runtime::Policy;
use fil_actors_runtime_v8::DealWeight;
use fvm_shared::bigint::{BigInt, Integer};
use fvm_shared::econ::TokenAmount;
use fvm_shared::piece::PaddedPieceSize;
use fvm_shared::sector::StoragePower;
use fvm_shared::TOTAL_FILECOIN;

use super::deal::DealProposal;

pub mod detail {
    use super::*;

    /// Maximum length of a deal label.
    pub const DEAL_MAX_LABEL_SIZE: usize = 256;

    /// Computes the weight for a deal proposal, which is a function of its size and duration.
    pub fn deal_weight(proposal: &DealProposal) -> DealWeight {
        let deal_duration = DealWeight::from(proposal.duration());
        deal_duration * proposal.piece_size.0
    }
}

pub fn deal_provider_collateral_bounds(
    policy: &Policy,
    size: PaddedPieceSize,
    network_raw_power: &StoragePower,
    baseline_power: &StoragePower,
    network_circulating_supply: &TokenAmount,
) -> (TokenAmount, TokenAmount) {
    // minimumProviderCollateral = ProviderCollateralSupplyTarget * normalizedCirculatingSupply
    // normalizedCirculatingSupply = networkCirculatingSupply * dealPowerShare
    // dealPowerShare = dealRawPower / max(BaselinePower(t), NetworkRawPower(t), dealRawPower)

    let lock_target_num = network_circulating_supply * policy.prov_collateral_percent_supply_num;
    let power_share_num = BigInt::from(size.0);
    let power_share_denom = max(max(network_raw_power, baseline_power), &power_share_num).clone();

    let num: BigInt = power_share_num * lock_target_num.atto();
    let denom: BigInt = power_share_denom * policy.prov_collateral_percent_supply_denom;

    (
        TokenAmount::from_atto(num.div_floor(&denom)),
        TOTAL_FILECOIN.clone(),
    )
}
