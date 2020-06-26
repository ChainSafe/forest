// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::network::EPOCHS_IN_YEAR;
use clock::ChainEpoch;
use fil_types::PaddedPieceSize;
use num_traits::Zero;
use vm::TokenAmount;

// The maximum supply of Filecoin that will ever exist (in token units)
const TOTAL_FILECOIN: u64 = 2_000_000_000;
const TOKEN_PRECISION: u64 = 1_000_000_000_000_000_000;

/// DealUpdatesInterval is the number of blocks between payouts for deals
pub const DEAL_UPDATED_INTERVAL: i64 = 100;

pub(super) fn deal_duration_bounds(_size: PaddedPieceSize) -> (ChainEpoch, ChainEpoch) {
    (0, EPOCHS_IN_YEAR) // PARAM_FINISH
}

pub(super) fn deal_price_per_epoch_bounds(
    _size: PaddedPieceSize,
    _duration: ChainEpoch,
) -> (TokenAmount, TokenAmount) {
    let v = TokenAmount::from(TOTAL_FILECOIN) * TokenAmount::from(TOKEN_PRECISION);
    (TokenAmount::zero(), v) // PARAM_FINISH
}

pub(super) fn deal_provider_collateral_bounds(
    _piece_size: PaddedPieceSize,
    _duration: ChainEpoch,
) -> (TokenAmount, TokenAmount) {
    let v = TokenAmount::from(TOTAL_FILECOIN) * TokenAmount::from(TOKEN_PRECISION);
    (TokenAmount::zero(), v) // PARAM_FINISH
}

pub(super) fn deal_client_collateral_bounds(
    _piece_size: PaddedPieceSize,
    _duration: ChainEpoch,
) -> (TokenAmount, TokenAmount) {
    let v = TokenAmount::from(TOTAL_FILECOIN) * TokenAmount::from(TOKEN_PRECISION);
    (TokenAmount::zero(), v) // PARAM_FINISH
}

pub(super) fn collateral_penalty_for_deal_activation_missed(
    provider_collateral: TokenAmount,
) -> TokenAmount {
    provider_collateral
}
