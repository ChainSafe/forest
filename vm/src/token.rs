// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use num_bigint::{biguint_ser, BigUint, ParseBigIntError};
use num_traits::CheckedSub;
use serde::{Deserialize, Serialize};
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
}

impl Add for TokenAmount {
    type Output = Self;

    fn add(self, other: TokenAmount) -> TokenAmount {
        Self(self.0 + other.0)
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

impl FromStr for TokenAmount {
    type Err = ParseBigIntError;

    fn from_str(src: &str) -> Result<Self, ParseBigIntError> {
        Ok(TokenAmount(BigUint::from_str(src)?))
    }
}
