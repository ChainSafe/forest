// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use multihash::{derive::Multihash, U32};

/// Multihash code for Poseidon BLS replica commitments.
pub const POSEIDON_BLS12_381_A1_FC1: u64 = 0xb401;

/// Multihash code for Sha2 256 trunc254 padded used in data commitments.
pub const SHA2_256_TRUNC254_PADDED: u64 = 0x1012;

/// Multihash generation codes for the Filecoin protocol. This is not an exhausting list of
/// codes used, just the ones used to generate multihashes.
#[derive(Clone, Copy, Debug, Eq, Multihash, PartialEq)]
#[mh(alloc_size = U32)]
pub enum Code {
    /// BLAKE2b-256 (32-byte hash size)
    #[mh(code = 0xb220, hasher = multihash::Blake2b256, digest = multihash::Blake2bDigest<U32>)]
    Blake2b256,

    /// Identity multihash (max 32 bytes)
    #[mh(code = 0x00, hasher = multihash::IdentityHasher::<U32>, digest = multihash::IdentityDigest<U32>)]
    Identity,
}
