// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use fvm_shared::econ::TokenAmount as TokenAmount_v2;
use fvm_shared3::econ::TokenAmount as TokenAmount_v3;
use num_bigint::BigInt;
use serde::{Deserialize, Serialize};
use std::ops::{Add, AddAssign, Deref, DerefMut, Mul};

// FIXME: Transparent Debug trait impl
// FIXME: Consider 'type TokenAmount = TokenAmount_v3'
#[derive(Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Debug, Default)]
#[serde(transparent)]
pub struct TokenAmount(TokenAmount_v3);

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

impl std::fmt::Display for TokenAmount {
    // This trait requires `fmt` with this exact signature.
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl TokenAmount {
    /// Returns the quantity of indivisible units.
    pub fn atto(&self) -> &BigInt {
        self.0.atto()
    }

    pub fn from_atto(atto: impl Into<BigInt>) -> Self {
        TokenAmount_v3::from_atto(atto).into()
    }

    #[inline]
    pub fn div_rem(&self, other: impl Into<BigInt>) -> (TokenAmount, TokenAmount) {
        let (q, r) = self.0.div_rem(other);
        (q.into(), r.into())
    }

    #[inline]
    pub fn div_ceil(&self, other: impl Into<BigInt>) -> TokenAmount {
        self.0.div_ceil(other).into()
    }

    #[inline]
    pub fn div_floor(&self, other: impl Into<BigInt>) -> TokenAmount {
        self.0.div_floor(other).into()
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

impl From<&TokenAmount_v2> for TokenAmount {
    fn from(other: &TokenAmount_v2) -> Self {
        TokenAmount::from(TokenAmount_v3::from_atto(other.atto().clone()))
    }
}

impl From<&TokenAmount_v3> for TokenAmount {
    fn from(other: &TokenAmount_v3) -> Self {
        TokenAmount(other.clone())
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

impl Mul<i64> for &TokenAmount {
    type Output = TokenAmount;
    fn mul(self, rhs: i64) -> Self::Output {
        (&self.0).mul(rhs).into()
    }
}

impl Add<TokenAmount> for &TokenAmount {
    type Output = TokenAmount;
    fn add(self, rhs: TokenAmount) -> Self::Output {
        (&self.0).add(rhs.0).into()
    }
}

impl AddAssign for TokenAmount {
    fn add_assign(&mut self, other: Self) {
        self.0.add_assign(other.0)
    }
}
