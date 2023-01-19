// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use fvm_shared::econ::TokenAmount as TokenAmount_v2;
use fvm_shared3::econ::TokenAmount as TokenAmount_v3;
use num_bigint::BigInt;
use serde::{Deserialize, Serialize};
use std::ops::{AddAssign, Deref, DerefMut, Mul};

// FIXME: Transparent Debug trait impl
#[derive(Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Debug, Default)]
#[serde(transparent)]
pub struct TokenAmount(TokenAmount_v3);

impl std::fmt::Display for TokenAmount {
    // This trait requires `fmt` with this exact signature.
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl TokenAmount {
    pub fn from_atto(atto: impl Into<BigInt>) -> Self {
        TokenAmount_v3::from_atto(atto).into()
    }
}

impl Deref for TokenAmount {
    type Target = TokenAmount_v3;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for TokenAmount {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl From<TokenAmount_v3> for TokenAmount {
    fn from(other: TokenAmount_v3) -> Self {
        TokenAmount(other)
    }
}

impl From<TokenAmount_v2> for TokenAmount {
    fn from(other: TokenAmount_v2) -> Self {
        TokenAmount::from(TokenAmount_v3::from_atto(other.atto().clone()))
    }
}

impl From<TokenAmount> for TokenAmount_v3 {
    fn from(other: TokenAmount) -> Self {
        other.0
    }
}

impl From<TokenAmount> for TokenAmount_v2 {
    fn from(other: TokenAmount) -> TokenAmount_v2 {
        TokenAmount_v2::from_atto(other.atto().clone())
    }
}

impl Mul<BigInt> for TokenAmount {
    type Output = TokenAmount;
    fn mul(self, rhs: BigInt) -> Self::Output {
        self.0.mul(rhs).into()
    }
}

impl Mul<BigInt> for &TokenAmount {
    type Output = TokenAmount;
    fn mul(self, rhs: BigInt) -> Self::Output {
        (&self.0).mul(rhs).into()
    }
}

impl AddAssign for TokenAmount {
    fn add_assign(&mut self, other: Self) {
        self.0.add_assign(other.0)
    }
}
