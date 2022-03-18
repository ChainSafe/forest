// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod mh_code;
mod prefix;

pub use self::mh_code::{Code, POSEIDON_BLS12_381_A1_FC1, SHA2_256_TRUNC254_PADDED};
pub use self::prefix::Prefix;
pub use cid::{Error, Version};
pub use multihash;
use multihash::Multihash;
use multihash::MultihashDigest;
use std::convert::TryFrom;

#[cfg(feature = "json")]
pub mod json;

/// Cbor [Cid] codec.
pub const DAG_CBOR: u64 = 0x71;
/// Sealed commitment [Cid] codec.
pub const FIL_COMMITMENT_SEALED: u64 = 0xf102;
/// Unsealed commitment [Cid] codec.
pub const FIL_COMMITMENT_UNSEALED: u64 = 0xf101;
/// Raw [Cid] codec. This represents data that is not encoded using any protocol.
pub const RAW: u64 = 0x55;

/// Constructs a cid with bytes using default version and codec
pub fn new_from_cbor(bz: &[u8], code: Code) -> Cid {
    let hash = code.digest(bz);
    Cid::new_v1(DAG_CBOR, hash)
}

/// Create a new CID from a prefix and some data.
pub fn new_from_prefix(prefix: &Prefix, data: &[u8]) -> Result<Cid, Error> {
    let hash: Multihash = Code::try_from(prefix.mh_type)?.digest(data);
    Cid::new(prefix.version, prefix.codec, hash)
}

pub use cid::Cid;
