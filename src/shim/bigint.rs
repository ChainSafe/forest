// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::fvm_shared_latest::bigint::bigint_ser;
use serde::{Deserialize, Serialize};

#[derive(
    Default,
    Clone,
    Debug,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    derive_more::Deref,
    derive_more::DerefMut,
)]
#[serde(transparent)]
pub struct BigInt(#[serde(with = "bigint_ser")] num_bigint::BigInt);

impl From<num_bigint::BigInt> for BigInt {
    fn from(other: num_bigint::BigInt) -> Self {
        BigInt(other)
    }
}
