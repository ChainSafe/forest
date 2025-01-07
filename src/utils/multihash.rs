// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//!
//! This module back-fills the Identify hasher and code that was removed in `multihash` crate.
//! See <https://github.com/multiformats/rust-multihash/blob/master/CHANGELOG.md#-breaking-changes>
//! and <https://github.com/multiformats/rust-multihash/pull/289>
//!

pub mod prelude {
    pub use super::MultihashCode;
    pub use multihash_codetable::MultihashDigest as _;
}

use multihash_derive::MultihashDigest;

/// Extends [`multihash_codetable::Code`] with `Identity`
#[derive(Clone, Copy, Debug, Eq, MultihashDigest, PartialEq)]
#[mh(alloc_size = 64)]
pub enum MultihashCode {
    #[mh(code = 0x0, hasher = IdentityHasher::<64>)]
    Identity,
    /// SHA-256 (32-byte hash size)
    #[mh(code = 0x12, hasher = multihash_codetable::Sha2_256)]
    Sha2_256,
    /// SHA-512 (64-byte hash size)
    #[mh(code = 0x13, hasher = multihash_codetable::Sha2_512)]
    Sha2_512,
    /// SHA3-224 (28-byte hash size)
    #[mh(code = 0x17, hasher = multihash_codetable::Sha3_224)]
    Sha3_224,
    /// SHA3-256 (32-byte hash size)
    #[mh(code = 0x16, hasher = multihash_codetable::Sha3_256)]
    Sha3_256,
    /// SHA3-384 (48-byte hash size)
    #[mh(code = 0x15, hasher = multihash_codetable::Sha3_384)]
    Sha3_384,
    /// SHA3-512 (64-byte hash size)
    #[mh(code = 0x14, hasher = multihash_codetable::Sha3_512)]
    Sha3_512,
    /// Keccak-224 (28-byte hash size)
    #[mh(code = 0x1a, hasher = multihash_codetable::Keccak224)]
    Keccak224,
    /// Keccak-256 (32-byte hash size)
    #[mh(code = 0x1b, hasher = multihash_codetable::Keccak256)]
    Keccak256,
    /// Keccak-384 (48-byte hash size)
    #[mh(code = 0x1c, hasher = multihash_codetable::Keccak384)]
    Keccak384,
    /// Keccak-512 (64-byte hash size)
    #[mh(code = 0x1d, hasher = multihash_codetable::Keccak512)]
    Keccak512,
    /// BLAKE2b-256 (32-byte hash size)
    #[mh(code = 0xb220, hasher = multihash_codetable::Blake2b256)]
    Blake2b256,
    /// BLAKE2b-512 (64-byte hash size)
    #[mh(code = 0xb240, hasher = multihash_codetable::Blake2b512)]
    Blake2b512,
    /// BLAKE2s-128 (16-byte hash size)
    #[mh(code = 0xb250, hasher = multihash_codetable::Blake2s128)]
    Blake2s128,
    /// BLAKE2s-256 (32-byte hash size)
    #[mh(code = 0xb260, hasher = multihash_codetable::Blake2s256)]
    Blake2s256,
    /// BLAKE3-256 (32-byte hash size)
    #[mh(code = 0x1e, hasher = multihash_codetable::Blake3_256)]
    Blake3_256,
    /// RIPEMD-160 (20-byte hash size)
    #[mh(code = 0x1053, hasher = multihash_codetable::Ripemd160)]
    Ripemd160,
    /// RIPEMD-256 (32-byte hash size)
    #[mh(code = 0x1054, hasher = multihash_codetable::Ripemd256)]
    Ripemd256,
    /// RIPEMD-320 (40-byte hash size)
    #[mh(code = 0x1055, hasher = multihash_codetable::Ripemd320)]
    Ripemd320,
}

/// Identity hasher with a maximum size.
///
/// # Panics
///
/// Panics if the input is bigger than the maximum size.
/// Ported from <https://github.com/multiformats/rust-multihash/pull/289>
#[derive(Debug)]
pub struct IdentityHasher<const S: usize> {
    i: usize,
    bytes: [u8; S],
}

impl<const S: usize> Default for IdentityHasher<S> {
    fn default() -> Self {
        Self {
            i: 0,
            bytes: [0u8; S],
        }
    }
}

impl<const S: usize> multihash_derive::Hasher for IdentityHasher<S> {
    fn update(&mut self, input: &[u8]) {
        let start = self.i.min(self.bytes.len());
        let end = (self.i + input.len()).min(self.bytes.len());
        self.bytes[start..end].copy_from_slice(input);
        self.i = end;
    }

    fn finalize(&mut self) -> &[u8] {
        &self.bytes[..self.i]
    }

    fn reset(&mut self) {
        self.i = 0
    }
}
