// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::ops::{Add, AddAssign, Deref, DerefMut, Mul, MulAssign, Sub, SubAssign};

use fvm_shared::econ::TokenAmount as TokenAmount_v2;
use fvm_shared3::econ::TokenAmount as TokenAmount_v3;
pub use fvm_shared3::{BLOCK_GAS_LIMIT, TOTAL_FILECOIN_BASE};
use lazy_static::lazy_static;
use num_bigint::BigInt;
use num_traits::Zero;
use serde::{Deserialize, Serialize};
use static_assertions::const_assert_eq;

const_assert_eq!(BLOCK_GAS_LIMIT, fvm_shared::BLOCK_GAS_LIMIT as u64);
const_assert_eq!(TOTAL_FILECOIN_BASE, fvm_shared::TOTAL_FILECOIN_BASE);

lazy_static! {
    /// Total Filecoin available to the network.
    pub static ref TOTAL_FILECOIN: TokenAmount = TokenAmount::from_whole(TOTAL_FILECOIN_BASE);
}

// FIXME: Transparent Debug trait impl
// FIXME: Consider 'type TokenAmount = TokenAmount_v3'
#[derive(Clone, PartialEq, Eq, Ord, PartialOrd, Hash, Serialize, Deserialize, Debug, Default)]
#[serde(transparent)]
pub struct TokenAmount(TokenAmount_v3);

#[cfg(test)]
impl quickcheck::Arbitrary for TokenAmount {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        use fvm_shared3::bigint::MAX_BIGINT_SIZE;
        use num::BigUint;
        // During serialization/deserialization, permissible length of the byte
        // representation (plus a leading positive sign byte for non-zero
        // values) of BigInts is currently set to a max of MAX_BIGINT_SIZE with
        // a value of 128; need to constrain the corresponding length during
        // `Arbitrary` generation of `BigInt` in `TokenAmount` to below
        // MAX_BIGINT_SIZE.
        // The 'significant_bits' variable changes the distribution from uniform
        // to log-scaled.
        let significant_bits = usize::arbitrary(g) % ((MAX_BIGINT_SIZE - 1) * 8);
        let bigint_upper_limit = BigUint::from(1u8) << significant_bits;
        TokenAmount::from_atto(BigUint::arbitrary(g) % bigint_upper_limit)
    }
}

impl Zero for TokenAmount {
    fn zero() -> Self {
        TokenAmount(TokenAmount_v3::zero())
    }
    fn is_zero(&self) -> bool {
        self.0.is_zero()
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

    pub fn from_nano(nano: impl Into<BigInt>) -> Self {
        TokenAmount_v3::from_nano(nano).into()
    }

    pub fn from_whole(fil: impl Into<BigInt>) -> Self {
        TokenAmount_v3::from_whole(fil).into()
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

impl From<&TokenAmount> for TokenAmount_v3 {
    fn from(other: &TokenAmount) -> TokenAmount_v3 {
        other.0.clone()
    }
}

impl From<TokenAmount> for TokenAmount_v2 {
    fn from(other: TokenAmount) -> TokenAmount_v2 {
        TokenAmount_v2::from_atto(other.atto().clone())
    }
}

impl From<&TokenAmount> for TokenAmount_v2 {
    fn from(other: &TokenAmount) -> TokenAmount_v2 {
        TokenAmount_v2::from_atto(other.atto().clone())
    }
}

impl Mul<BigInt> for TokenAmount {
    type Output = TokenAmount;
    fn mul(self, rhs: BigInt) -> Self::Output {
        self.0.mul(rhs).into()
    }
}

impl MulAssign<BigInt> for TokenAmount {
    fn mul_assign(&mut self, rhs: BigInt) {
        self.0.mul_assign(rhs)
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

impl Mul<i64> for TokenAmount {
    type Output = TokenAmount;
    fn mul(self, rhs: i64) -> Self::Output {
        (&self.0).mul(rhs).into()
    }
}

impl Mul<u64> for &TokenAmount {
    type Output = TokenAmount;
    fn mul(self, rhs: u64) -> Self::Output {
        (&self.0).mul(rhs).into()
    }
}

impl Mul<u64> for TokenAmount {
    type Output = TokenAmount;
    fn mul(self, rhs: u64) -> Self::Output {
        (&self.0).mul(rhs).into()
    }
}

impl Add<TokenAmount> for &TokenAmount {
    type Output = TokenAmount;
    fn add(self, rhs: TokenAmount) -> Self::Output {
        (&self.0).add(rhs.0).into()
    }
}

impl Add<&TokenAmount> for &TokenAmount {
    type Output = TokenAmount;
    fn add(self, rhs: &TokenAmount) -> Self::Output {
        (&self.0).add(&rhs.0).into()
    }
}

impl Add<TokenAmount> for TokenAmount {
    type Output = TokenAmount;
    fn add(self, rhs: TokenAmount) -> Self::Output {
        (&self.0).add(rhs.0).into()
    }
}

impl Add<&TokenAmount> for TokenAmount {
    type Output = TokenAmount;
    fn add(self, rhs: &TokenAmount) -> Self::Output {
        (&self.0).add(&rhs.0).into()
    }
}

impl AddAssign for TokenAmount {
    fn add_assign(&mut self, other: Self) {
        self.0.add_assign(other.0)
    }
}

impl SubAssign for TokenAmount {
    fn sub_assign(&mut self, other: Self) {
        self.0.sub_assign(other.0)
    }
}

impl Sub<&TokenAmount> for TokenAmount {
    type Output = TokenAmount;
    fn sub(self, rhs: &TokenAmount) -> Self::Output {
        (&self.0).sub(&rhs.0).into()
    }
}

impl Sub<TokenAmount> for &TokenAmount {
    type Output = TokenAmount;
    fn sub(self, rhs: TokenAmount) -> Self::Output {
        (&self.0).sub(&rhs.0).into()
    }
}
