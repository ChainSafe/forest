// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use clock::ChainEpoch;
use fil_types::PaddedPieceSize;
use num_traits::Zero;
use vm::TokenAmount;

// The maximum supply of Filecoin that will ever exist (in token units)
const TOTAL_FILECOIN: u32 = 2_000_000_000;

pub(super) fn deal_duration_bounds(_size: PaddedPieceSize) -> (ChainEpoch, ChainEpoch) {
    (0, 10000) // PARAM_FINISH
}

pub(super) fn deal_price_per_epoch_bounds(
    _size: PaddedPieceSize,
    _duration: ChainEpoch,
) -> (TokenAmount, TokenAmount) {
    (TokenAmount::zero(), TokenAmount::from(TOTAL_FILECOIN)) // PARAM_FINISH
}

pub(super) fn deal_provider_collateral_bounds(
    _piece_size: PaddedPieceSize,
    _duration: ChainEpoch,
) -> (TokenAmount, TokenAmount) {
    (TokenAmount::zero(), TokenAmount::from(TOTAL_FILECOIN)) // PARAM_FINISH
}

pub(super) fn deal_client_collateral_bounds(
    _piece_size: PaddedPieceSize,
    _duration: ChainEpoch,
) -> (TokenAmount, TokenAmount) {
    (TokenAmount::zero(), TokenAmount::from(TOTAL_FILECOIN)) // PARAM_FINISH
}

pub(super) fn collateral_penalty_for_deal_activation_missed(
    provider_collateral: TokenAmount,
) -> TokenAmount {
    provider_collateral
}
