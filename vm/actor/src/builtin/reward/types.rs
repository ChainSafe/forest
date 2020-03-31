// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use num_bigint::BigInt;

/// Number of token units in an abstract "FIL" token.
/// The network works purely in the indivisible token amounts. This constant converts to a fixed decimal with more
/// human-friendly scale.
pub const TOKEN_PRECISION: i64 = 1_000_000_000_000_000_000;

lazy_static! {
    /// Target reward released to each block winner.
    pub static ref BLOCK_REWARD_TARGET: BigInt = BigInt::from(100) * TOKEN_PRECISION;
}
