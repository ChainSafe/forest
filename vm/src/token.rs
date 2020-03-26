// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use num_bigint::{biguint_ser, BigInt, BigUint, ParseBigIntError, Sign};
use num_traits::{CheckedSub, Signed};
use serde::{Deserialize, Serialize};
use std::convert::TryFrom;
use std::fmt;
use std::ops::{Add, AddAssign, Sub};
use std::str::FromStr;

/// Wrapper around a big int variable to handle token specific functionality
// TODO verify on finished spec whether or not big int or uint
#[derive(Default, Clone, PartialEq, Debug, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct TokenAmount(#[serde(with = "biguint_ser")] pub BigUint);

impl TokenAmount {
    pub fn new(val: u64) -> Self {
        TokenAmount(BigUint::from(val))
    }
    /// Utility function just to be able to do arithmetic with negative bigint values
    /// To match actor spec
    pub fn checked_add_bigint(&self, other: &BigInt) -> Option<TokenAmount> {
        match other.sign() {
            Sign::Minus => self.checked_sub(&TokenAmount::try_from(other.abs()).unwrap()),
            _ => Some(TokenAmount::try_from(BigInt::from(self.0.clone()) + other).unwrap()),
        }
    }
}

impl Add for TokenAmount {
    type Output = Self;

    fn add(self, other: TokenAmount) -> TokenAmount {
        Self(self.0 + other.0)
    }
}

impl Add<&TokenAmount> for TokenAmount {
    type Output = Self;

    fn add(self, other: &TokenAmount) -> TokenAmount {
        Self(self.0 + &other.0)
    }
}

impl AddAssign for TokenAmount {
    fn add_assign(&mut self, other: TokenAmount) {
        self.0.add_assign(other.0)
    }
}

impl AddAssign<&TokenAmount> for TokenAmount {
    fn add_assign(&mut self, other: &TokenAmount) {
        self.0.add_assign(&other.0)
    }
}

impl Sub for TokenAmount {
    type Output = Self;

    fn sub(self, other: TokenAmount) -> TokenAmount {
        Self(self.0 - other.0)
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

impl From<TokenAmount> for BigInt {
    fn from(t: TokenAmount) -> Self {
        Self::from(t.0)
    }
}

impl FromStr for TokenAmount {
    type Err = ParseBigIntError;

    fn from_str(src: &str) -> Result<Self, ParseBigIntError> {
        Ok(TokenAmount(BigUint::from_str(src)?))
    }
}
