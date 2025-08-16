// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::fvm_shared_latest::bigint::bigint_ser;
use get_size2::GetSize;
use serde::{Deserialize, Serialize};
use std::ops::{Deref, DerefMut};

#[derive(Default, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct BigInt(#[serde(with = "bigint_ser")] num::BigInt);

impl GetSize for BigInt {
    fn get_heap_size(&self) -> usize {
        self.0.bits().div_ceil(8) as _
    }
}

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
