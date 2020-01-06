// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

use num_bigint::BigUint;

/// Wrapper around a big int variable to handle token specific functionality
// TODO verify on finished spec whether or not big int or uint
#[derive(Default, Clone, PartialEq, Debug)]
pub struct TokenAmount(pub BigUint);

impl TokenAmount {
    pub fn new(val: u64) -> Self {
        TokenAmount(BigUint::from(val))
    }
}
