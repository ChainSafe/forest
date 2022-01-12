// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod bigint_ser;
pub mod biguint_ser;

pub use num_bigint::*;
pub use num_integer::{self, Integer};

/// MAX_ENCODED_SIZE is the max length of a byte slice representing a
/// CBOR serialized BigInt or BigUint.
const MAX_ENCODED_SIZE: usize = 128;
