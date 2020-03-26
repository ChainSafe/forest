// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use num_bigint::{biguint_ser, BigInt, BigUint, ParseBigIntError};
use num_traits::CheckedSub;
use serde::{Deserialize, Serialize};
use std::convert::TryFrom;
use std::fmt;
use std::ops::{Add, AddAssign, Mul, Sub};
use std::str::FromStr;

/// Wrapper around a big int variable to handle token specific functionality
// TODO verify on finished spec whether or not big int or uint
#[derive(Default, Clone, PartialEq, Debug, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct TokenAmount(#[serde(with = "biguint_ser")] pub BigUint);

impl TokenAmount {
    pub fn new(val: u64) -> Self {
        TokenAmount(BigUint::from(val))
    }

    pub fn add_bigint(&self, other: BigInt) -> Result<TokenAmount, &'static str> {
        let new_total = BigInt::from(self.0.clone()) + other;
        TokenAmount::try_from(new_total)
    }
}

impl Add for TokenAmount {
    type Output = Self;

    fn add(self, other: TokenAmount) -> TokenAmount {
        Self(self.0 + other.0)
    }
}

impl<'a> Add<&'a TokenAmount> for TokenAmount {
    type Output = Self;

    #[inline]
    fn add(self, other: &TokenAmount) -> TokenAmount {
        TokenAmount(self.0 + &other.0)
    }
}

impl AddAssign for TokenAmount {
    fn add_assign(&mut self, other: TokenAmount) {
        self.0.add_assign(other.0)
    }
}

impl Sub for TokenAmount {
    type Output = Self;

    fn sub(self, other: TokenAmount) -> TokenAmount {
        Self(self.0 - other.0)
    }
}

impl<'a> Sub<&'a TokenAmount> for &TokenAmount {
    type Output = TokenAmount;

    fn sub(self, other: &TokenAmount) -> TokenAmount {
        TokenAmount(&self.0 - &other.0)
    }
}

impl Mul<u64> for &TokenAmount {
    type Output = TokenAmount;

    fn mul(self, rhs: u64) -> TokenAmount {
        TokenAmount(&self.0 * rhs)
    }
}

impl CheckedSub for TokenAmount {
    fn checked_sub(&self, other: &Self) -> Option<Self> {
        self.0.checked_sub(&other.0).map(TokenAmount)
    }
}

impl fmt::Display for TokenAmount {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "TokenAmount({})", self.0)
    }
}

impl TryFrom<BigInt> for TokenAmount {
    type Error = &'static str;

    fn try_from(value: BigInt) -> Result<Self, Self::Error> {
        Ok(TokenAmount(
            value.to_biguint().ok_or("TokenAmount cannot be negative")?,
        ))
    }
}

impl FromStr for TokenAmount {
    type Err = ParseBigIntError;

    fn from_str(src: &str) -> Result<Self, ParseBigIntError> {
        Ok(TokenAmount(BigUint::from_str(src)?))
    }
}
