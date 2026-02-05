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
    derive_more::From,
)]
#[serde(transparent)]
pub struct BigInt(#[serde(with = "bigint_ser")] num_bigint::BigInt);
