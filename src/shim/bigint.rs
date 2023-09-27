// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::ops::{Deref, DerefMut};

use super::fvm_shared_latest::bigint::bigint_ser;
pub use super::fvm_shared_latest::bigint::bigint_ser::{BigIntDe, BigIntSer};
use serde::{Deserialize, Serialize};

#[derive(Default, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct BigInt(#[serde(with = "bigint_ser")] num_bigint::BigInt);

impl Deref for BigInt {
    type Target = num_bigint::BigInt;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for BigInt {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl From<num_bigint::BigInt> for BigInt {
    fn from(other: num_bigint::BigInt) -> Self {
        BigInt(other)
    }
}
