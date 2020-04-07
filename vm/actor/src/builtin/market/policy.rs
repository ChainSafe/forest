// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use clock::ChainEpoch;
use num_traits::Zero;
use vm::{PaddedPieceSize, TokenAmount};

// TODO remove allow dead_code on actor impl

#[allow(dead_code)]
pub(super) fn deal_duration_bounds(_size: PaddedPieceSize) -> (ChainEpoch, ChainEpoch) {
    (0, 10000) // PARAM_FINISH
}

#[allow(dead_code)]
pub(super) fn deal_price_per_epoch_bounds(
    _size: PaddedPieceSize,
    _duration: ChainEpoch,
) -> (TokenAmount, TokenAmount) {
    (TokenAmount::zero(), TokenAmount::from(1u32 << 20)) // PARAM_FINISH
}

#[allow(dead_code)]
pub(super) fn deal_provider_collateral_bounds(
    _piece_size: PaddedPieceSize,
    _duration: ChainEpoch,
) -> (TokenAmount, TokenAmount) {
    (TokenAmount::zero(), TokenAmount::from(1u32 << 20)) // PARAM_FINISH
}

#[allow(dead_code)]
pub(super) fn deal_client_collateral_bounds(
    _piece_size: PaddedPieceSize,
    _duration: ChainEpoch,
) -> (TokenAmount, TokenAmount) {
    (TokenAmount::zero(), TokenAmount::from(1u32 << 20)) // PARAM_FINISH
}

pub(super) fn collateral_penalty_for_deal_activation_missed(
    provider_collateral: TokenAmount,
) -> TokenAmount {
    provider_collateral
}
