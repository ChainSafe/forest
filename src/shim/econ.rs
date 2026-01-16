// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::fvm_shared_latest::econ::TokenAmount as TokenAmount_latest;
use crate::utils::get_size::big_int_heap_size_helper;
use fvm_shared2::econ::TokenAmount as TokenAmount_v2;
use fvm_shared3::econ::TokenAmount as TokenAmount_v3;
pub use fvm_shared3::{BLOCK_GAS_LIMIT, TOTAL_FILECOIN_BASE};
use fvm_shared4::econ::TokenAmount as TokenAmount_v4;
use get_size2::GetSize;
use num_bigint::BigInt;
use num_traits::{One, Signed, Zero};
use serde::{Deserialize, Serialize};
use static_assertions::const_assert_eq;
use std::{
    ops::{Add, AddAssign, Div, Mul, MulAssign, Neg, Rem, Sub, SubAssign},
    sync::LazyLock,
};

const_assert_eq!(BLOCK_GAS_LIMIT, fvm_shared2::BLOCK_GAS_LIMIT as u64);
const_assert_eq!(TOTAL_FILECOIN_BASE, fvm_shared2::TOTAL_FILECOIN_BASE);

/// Total Filecoin available to the network.
pub static TOTAL_FILECOIN: LazyLock<TokenAmount> =
    LazyLock::new(|| TokenAmount::from_whole(TOTAL_FILECOIN_BASE));

#[derive(
    Clone,
    PartialEq,
    Eq,
    Ord,
    PartialOrd,
    Hash,
    Serialize,
    Deserialize,
    Default,
    derive_more::Deref,
    derive_more::DerefMut,
    derive_more::Debug,
    derive_more::Display,
    derive_more::From,
    derive_more::Into,
)]
#[serde(transparent)]
pub struct TokenAmount(TokenAmount_latest);

impl GetSize for TokenAmount {
    fn get_heap_size(&self) -> usize {
        big_int_heap_size_helper(self.0.atto())
    }
}

#[cfg(test)]
impl quickcheck::Arbitrary for TokenAmount {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        use fvm_shared4::bigint::MAX_BIGINT_SIZE;
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
        TokenAmount(TokenAmount_latest::zero())
    }
    fn is_zero(&self) -> bool {
        self.0.is_zero()
    }
}

impl One for TokenAmount {
    fn one() -> Self {
        TokenAmount::from_atto(1)
    }
}

impl num_traits::Num for TokenAmount {
    type FromStrRadixErr = num_bigint::ParseBigIntError;

    fn from_str_radix(str: &str, radix: u32) -> Result<Self, Self::FromStrRadixErr> {
        Ok(Self::from_atto(BigInt::from_str_radix(str, radix)?))
    }
}

impl Neg for TokenAmount {
    type Output = Self;

    fn neg(self) -> Self::Output {
        self.0.neg().into()
    }
}

impl Neg for &TokenAmount {
    type Output = TokenAmount;

    fn neg(self) -> Self::Output {
        (&self.0).neg().into()
    }
}

impl TokenAmount {
    /// The logical number of decimal places of a token unit.
    pub const DECIMALS: usize = TokenAmount_latest::DECIMALS;

    /// The logical precision of a token unit.
    pub const PRECISION: u64 = TokenAmount_latest::PRECISION;

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

    /// Checks if two `TokenAmounts` are within a percentage delta of each other.
    /// This method computes the absolute difference between `self` and `other`, then checks
    /// if this difference is within `delta_percent` of the larger of the two values.
    /// # Arguments
    /// * `other` - The value to compare against
    /// * `delta_percent` - The allowed percentage difference relative to the larger value (e.g., 5 for 5%)
    ///
    /// # Returns
    /// `true` if the values are within the delta, `false` otherwise
    pub fn is_within_percent(&self, other: &TokenAmount, delta_percent: u64) -> bool {
        match (self.is_zero(), other.is_zero()) {
            (true, true) => return true,                   // Both zero: equal
            (true, false) | (false, true) => return false, // One zero: fundamentally different
            _ => {}                                        // Both non-zero: continue
        }

        let diff = (self - other).abs();
        let max_magnitude = self.abs().max(other.abs());
        let threshold = (max_magnitude * delta_percent).div_floor(100u64);

        diff <= threshold
    }
}

impl From<TokenAmount> for BigInt {
    fn from(value: TokenAmount) -> Self {
        value.atto().to_owned()
    }
}

impl From<BigInt> for TokenAmount {
    fn from(value: BigInt) -> Self {
        Self::from_atto(value)
    }
}

impl From<TokenAmount_v2> for TokenAmount {
    fn from(other: TokenAmount_v2) -> Self {
        (&other).into()
    }
}

impl From<&TokenAmount_v2> for TokenAmount {
    fn from(other: &TokenAmount_v2) -> Self {
        Self(TokenAmount_latest::from_atto(other.atto().clone()))
    }
}

impl From<&TokenAmount_v3> for TokenAmount {
    fn from(other: &TokenAmount_v3) -> Self {
        Self(TokenAmount_latest::from_atto(other.atto().clone()))
    }
}

impl From<TokenAmount_v3> for TokenAmount {
    fn from(other: TokenAmount_v3) -> Self {
        (&other).into()
    }
}

impl From<&TokenAmount_v4> for TokenAmount {
    fn from(other: &TokenAmount_v4) -> Self {
        other.clone().into()
    }
}

impl From<TokenAmount> for TokenAmount_v2 {
    fn from(other: TokenAmount) -> Self {
        (&other).into()
    }
}

impl From<&TokenAmount> for TokenAmount_v2 {
    fn from(other: &TokenAmount) -> Self {
        Self::from_atto(other.atto().clone())
    }
}

impl From<TokenAmount> for TokenAmount_v3 {
    fn from(other: TokenAmount) -> Self {
        (&other).into()
    }
}

impl From<&TokenAmount> for TokenAmount_v3 {
    fn from(other: &TokenAmount) -> Self {
        Self::from_atto(other.atto().clone())
    }
}

impl From<&TokenAmount> for TokenAmount_v4 {
    fn from(other: &TokenAmount) -> Self {
        other.0.clone()
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

/// Macro to implement binary operators for `TokenAmount`.
macro_rules! impl_token_amount_op {
    ($trait:ident, $method:ident) => {
        impl $trait<TokenAmount> for TokenAmount {
            type Output = TokenAmount;
            #[inline]
            fn $method(self, rhs: TokenAmount) -> Self::Output {
                self.atto().$method(rhs.atto()).into()
            }
        }

        impl $trait<&TokenAmount> for TokenAmount {
            type Output = TokenAmount;
            #[inline]
            fn $method(self, rhs: &TokenAmount) -> Self::Output {
                self.atto().$method(rhs.atto()).into()
            }
        }

        impl $trait<TokenAmount> for &TokenAmount {
            type Output = TokenAmount;
            #[inline]
            fn $method(self, rhs: TokenAmount) -> Self::Output {
                self.atto().$method(rhs.atto()).into()
            }
        }

        impl $trait<&TokenAmount> for &TokenAmount {
            type Output = TokenAmount;
            #[inline]
            fn $method(self, rhs: &TokenAmount) -> Self::Output {
                self.atto().$method(rhs.atto()).into()
            }
        }
    };
}

impl_token_amount_op!(Add, add);
impl_token_amount_op!(Sub, sub);
impl_token_amount_op!(Mul, mul);
impl_token_amount_op!(Div, div);
impl_token_amount_op!(Rem, rem);

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

impl Signed for TokenAmount {
    fn abs(&self) -> Self {
        self.0.atto().abs().into()
    }

    fn abs_sub(&self, other: &Self) -> Self {
        self.0.atto().abs_sub(other.0.atto()).into()
    }

    fn signum(&self) -> Self {
        self.0.atto().signum().into()
    }

    fn is_positive(&self) -> bool {
        self.0.is_positive()
    }

    fn is_negative(&self) -> bool {
        self.0.is_negative()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use num_traits::Signed;

    #[test]
    fn test_abs_positive() {
        let val = TokenAmount::from_atto(100);
        assert_eq!(val.abs(), val);
    }

    #[test]
    fn test_abs_negative() {
        let val = TokenAmount::from_atto(-100);
        let expected = TokenAmount::from_atto(100);
        assert_eq!(val.abs(), expected);
    }

    #[test]
    fn test_abs_zero() {
        let val = TokenAmount::zero();
        assert_eq!(val.abs(), val);
    }

    #[test]
    fn test_signum_positive() {
        let val = TokenAmount::from_atto(100);
        assert_eq!(val.signum(), TokenAmount::one());
    }

    #[test]
    fn test_signum_negative() {
        let val = TokenAmount::from_atto(-100);
        assert_eq!(val.signum(), -TokenAmount::one());
    }

    #[test]
    fn test_signum_zero() {
        let val = TokenAmount::zero();
        assert_eq!(val.signum(), TokenAmount::zero());
    }

    #[test]
    fn test_is_positive() {
        assert!(TokenAmount::from_atto(1).is_positive());
        assert!(TokenAmount::from_atto(100).is_positive());
        assert!(!TokenAmount::from_atto(-1).is_positive());
        assert!(!TokenAmount::zero().is_positive());
    }

    #[test]
    fn test_is_negative() {
        assert!(TokenAmount::from_atto(-1).is_negative());
        assert!(TokenAmount::from_atto(-100).is_negative());
        assert!(!TokenAmount::from_atto(1).is_negative());
        assert!(!TokenAmount::zero().is_negative());
    }

    #[test]
    fn test_abs_sub() {
        let val1 = TokenAmount::from_atto(100);
        let val2 = TokenAmount::from_atto(70);
        assert_eq!(val1.abs_sub(&val2), TokenAmount::from_atto(30));
        assert_eq!(val2.abs_sub(&val1), TokenAmount::zero());
    }

    #[test]
    fn test_neg_trait() {
        let val = TokenAmount::from_atto(100);
        assert_eq!(-val.clone(), TokenAmount::from_atto(-100));
        assert_eq!(-(-val), TokenAmount::from_atto(100));
    }

    #[test]
    fn test_is_within_percent_zero_edge_cases() {
        let zero = TokenAmount::zero();
        assert!(zero.is_within_percent(&zero, 5));

        let val = TokenAmount::from_atto(100);
        assert!(!val.is_within_percent(&zero, 5));
        assert!(!zero.is_within_percent(&val, 5));
    }

    #[test]
    fn test_is_within_percent_boundary_conditions() {
        let base = TokenAmount::from_atto(100);

        // exactly at threshold
        let at_threshold = TokenAmount::from_atto(105);
        assert!(base.is_within_percent(&at_threshold, 5));
        assert!(at_threshold.is_within_percent(&base, 5));

        // over threshold
        let over_threshold = TokenAmount::from_atto(106);
        assert!(!base.is_within_percent(&over_threshold, 5));
        assert!(!over_threshold.is_within_percent(&base, 5));

        // below base
        let below = TokenAmount::from_atto(95);
        assert!(base.is_within_percent(&below, 5));

        // different thresholds (3% difference)
        let val1 = TokenAmount::from_atto(1000);
        let val2 = TokenAmount::from_atto(1030);
        assert!(val1.is_within_percent(&val2, 5));
        assert!(val1.is_within_percent(&val2, 3));
        assert!(!val1.is_within_percent(&val2, 2));
    }

    #[test]
    fn test_is_within_percent_large_values() {
        let val1 = TokenAmount::from_atto(1_500_000_000_000_000u64);
        let val2 = TokenAmount::from_atto(1_570_000_000_000_000u64);
        assert!(val1.is_within_percent(&val2, 5));
        assert!(!val1.is_within_percent(&val2, 4));
    }

    #[test]
    fn test_is_within_percent_negative_values() {
        let neg1 = TokenAmount::from_atto(-100);
        let neg2 = TokenAmount::from_atto(-95);
        assert!(neg1.is_within_percent(&neg2, 5));
        assert!(neg2.is_within_percent(&neg1, 5));

        // over threshold
        let neg3 = TokenAmount::from_atto(-94);
        assert!(!neg1.is_within_percent(&neg3, 5));
        assert!(!neg3.is_within_percent(&neg1, 5));
    }

    #[test]
    fn test_is_within_percent_mixed_signs() {
        let pos = TokenAmount::from_atto(100);
        let neg = TokenAmount::from_atto(-100);
        assert!(!pos.is_within_percent(&neg, 5));
        assert!(!pos.is_within_percent(&neg, 100));
        assert!(pos.is_within_percent(&neg, 200));
    }

    #[test]
    fn test_div_rem() {
        let dividend = TokenAmount::from_atto(100);
        let divisor = TokenAmount::from_atto(30);
        let quotient = dividend.clone() / divisor.clone();
        let remainder = dividend.clone() % divisor.clone();

        assert_eq!(quotient, TokenAmount::from_atto(3));
        assert_eq!(remainder, TokenAmount::from_atto(10));
    }

    #[test]
    fn test_one_trait() {
        assert_eq!(TokenAmount::one(), TokenAmount::from_atto(1));
    }
}
