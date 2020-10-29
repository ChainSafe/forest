// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::deal::DealProposal;
use crate::network::{
    DEAL_WEIGHT_MULTIPLIER, EPOCHS_IN_DAY, QUALITY_BASE_MULTIPLIER, SECTOR_QUALITY_PRECISION,
    VERIFIED_DEAL_WEIGHT_MULTIPLIER,
};
use crate::DealWeight;
use clock::ChainEpoch;
use fil_types::{NetworkVersion, PaddedPieceSize, StoragePower, TOTAL_FILECOIN};
use num_bigint::Integer;
use num_traits::Zero;
use std::cmp::max;
use vm::TokenAmount;

/// DealUpdatesInterval is the number of blocks between payouts for deals
pub const DEAL_UPDATES_INTERVAL: i64 = EPOCHS_IN_DAY;

/// Numerator of the percentage of normalized cirulating
/// supply that must be covered by provider collateral
const PROV_COLLATERAL_PERCENT_SUPPLY_NUM_V0: i64 = 5;
const PROV_COLLATERAL_PERCENT_SUPPLY_NUM_V1: i64 = 1;

/// Denominator of the percentage of normalized cirulating
/// supply that must be covered by provider collateral
const PROV_COLLATERAL_PERCENT_SUPPLY_DENOM: i64 = 100;

/// Bounds (inclusive) on deal duration.
pub(super) fn deal_duration_bounds(_size: PaddedPieceSize) -> (ChainEpoch, ChainEpoch) {
    (180 * EPOCHS_IN_DAY, 540 * EPOCHS_IN_DAY)
}

pub(super) fn deal_price_per_epoch_bounds(
    _size: PaddedPieceSize,
    _duration: ChainEpoch,
) -> (TokenAmount, &'static TokenAmount) {
    (0.into(), &TOTAL_FILECOIN)
}

pub(super) fn deal_provider_collateral_bounds(
    size: PaddedPieceSize,
    verified: bool,
    network_raw_power: &StoragePower,
    network_qa_power: &StoragePower,
    baseline_power: &StoragePower,
    network_circulating_supply: &TokenAmount,
    network_version: NetworkVersion,
) -> (TokenAmount, &'static TokenAmount) {
    // minimumProviderCollateral = (ProvCollateralPercentSupplyNum / ProvCollateralPercentSupplyDenom) * normalizedCirculatingSupply
    // normalizedCirculatingSupply = FILCirculatingSupply * dealPowerShare
    // dealPowerShare = dealQAPower / max(BaselinePower(t), NetworkQAPower(t), dealQAPower)

    let (lock_target_num, power_share_num, power_share_denom) = if network_version
        < NetworkVersion::V1
    {
        let lock_target_num = network_circulating_supply * PROV_COLLATERAL_PERCENT_SUPPLY_NUM_V0;
        let power_share_num = deal_qa_power(size, verified);
        let power_share_denom =
            max(max(network_qa_power, baseline_power), &power_share_num).clone();
        (lock_target_num, power_share_num, power_share_denom)
    } else {
        let lock_target_num = network_circulating_supply * PROV_COLLATERAL_PERCENT_SUPPLY_NUM_V1;
        let power_share_num = TokenAmount::from(size.0);
        let power_share_denom =
            max(max(network_raw_power, baseline_power), &power_share_num).clone();
        (lock_target_num, power_share_num, power_share_denom)
    };

    let num: TokenAmount = power_share_num * lock_target_num;
    let denom: TokenAmount = power_share_denom * PROV_COLLATERAL_PERCENT_SUPPLY_DENOM;
    ((num.div_floor(&denom)), &TOTAL_FILECOIN)
}

pub(super) fn deal_client_collateral_bounds(
    _piece_size: PaddedPieceSize,
    _duration: ChainEpoch,
) -> (TokenAmount, &'static TokenAmount) {
    (TokenAmount::zero(), &TOTAL_FILECOIN) // PARAM_FINISH
}

/// Penalty to provider deal collateral if the deadline expires before sector commitment.
pub(super) fn collateral_penalty_for_deal_activation_missed(
    provider_collateral: TokenAmount,
) -> TokenAmount {
    provider_collateral
}

/// Computes the weight for a deal proposal, which is a function of its size and duration.
pub(super) fn deal_weight(proposal: &DealProposal) -> DealWeight {
    let deal_duration = DealWeight::from(proposal.duration());
    deal_duration * proposal.piece_size.0
}

pub(super) fn deal_qa_power(deal_size: PaddedPieceSize, verified: bool) -> DealWeight {
    let scaled_up_quality = if verified {
        (VERIFIED_DEAL_WEIGHT_MULTIPLIER.clone() << SECTOR_QUALITY_PRECISION)
            .div_floor(&QUALITY_BASE_MULTIPLIER)
    } else {
        (DEAL_WEIGHT_MULTIPLIER.clone() << SECTOR_QUALITY_PRECISION)
            .div_floor(&QUALITY_BASE_MULTIPLIER)
    };
    let scaled_up_qa_power = scaled_up_quality * deal_size.0;
    scaled_up_qa_power >> SECTOR_QUALITY_PRECISION
}
