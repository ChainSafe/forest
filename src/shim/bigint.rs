// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::fvm_shared_latest::bigint::{MAX_BIGINT_SIZE, bigint_ser};
use serde::{Deserialize, Serialize};

/// Largest magnitude, in bits, that fits the CBOR encoding (one byte is the sign).
pub const MAX_BIGINT_BITS: u64 = (MAX_BIGINT_SIZE as u64 - 1) * 8;

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
