// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod mh_code;

pub use self::mh_code::{Code, POSEIDON_BLS12_381_A1_FC1, SHA2_256_TRUNC254_PADDED};
pub use cid::{Error, Version};
pub use multihash;
use multihash::MultihashDigest;

pub use fvm_ipld_encoding::DAG_CBOR;
pub use fvm_shared::commcid::FIL_COMMITMENT_SEALED;
pub use fvm_shared::commcid::FIL_COMMITMENT_UNSEALED;
pub use fvm_shared::IPLD_RAW as RAW;

#[cfg(feature = "json")]
pub mod json;

/// Constructs a cid with bytes using default version and codec
pub fn new_from_cbor(bz: &[u8], code: Code) -> Cid {
    let hash = code.digest(bz);
    Cid::new_v1(DAG_CBOR, hash)
}

pub use cid::Cid;
