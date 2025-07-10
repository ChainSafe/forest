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

use multihash_derive::{Hasher, MultihashDigest};

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

impl MultihashCode {
    /// Calculate the [`Multihash`] of the input byte stream.
    pub fn digest_byte_stream<R: std::io::Read>(&self, bytes: &mut R) -> anyhow::Result<Multihash> {
        fn hash<'a, H: Hasher, R: std::io::Read>(
            hasher: &'a mut H,
            bytes: &'a mut R,
        ) -> anyhow::Result<&'a [u8]> {
            let mut buf = [0; 1024];
            loop {
                let n = bytes.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                if let Some(b) = buf.get(0..n) {
                    hasher.update(b);
                }
            }
            Ok(hasher.finalize())
        }

        Ok(match self {
            Self::Sha2_256 => {
                let mut hasher = multihash_codetable::Sha2_256::default();
                self.wrap(hash(&mut hasher, bytes)?)?
            }
            Self::Sha2_512 => {
                let mut hasher = multihash_codetable::Sha2_512::default();
                self.wrap(hash(&mut hasher, bytes)?)?
            }
            Self::Sha3_224 => {
                let mut hasher = multihash_codetable::Sha3_224::default();
                self.wrap(hash(&mut hasher, bytes)?)?
            }
            Self::Sha3_256 => {
                let mut hasher = multihash_codetable::Sha3_256::default();
                self.wrap(hash(&mut hasher, bytes)?)?
            }
            Self::Sha3_384 => {
                let mut hasher = multihash_codetable::Sha3_384::default();
                self.wrap(hash(&mut hasher, bytes)?)?
            }
            Self::Sha3_512 => {
                let mut hasher = multihash_codetable::Sha3_512::default();
                self.wrap(hash(&mut hasher, bytes)?)?
            }
            Self::Keccak224 => {
                let mut hasher = multihash_codetable::Keccak224::default();
                self.wrap(hash(&mut hasher, bytes)?)?
            }
            Self::Keccak256 => {
                let mut hasher = multihash_codetable::Keccak256::default();
                self.wrap(hash(&mut hasher, bytes)?)?
            }
            Self::Keccak384 => {
                let mut hasher = multihash_codetable::Keccak384::default();
                self.wrap(hash(&mut hasher, bytes)?)?
            }
            Self::Keccak512 => {
                let mut hasher = multihash_codetable::Keccak512::default();
                self.wrap(hash(&mut hasher, bytes)?)?
            }
            Self::Blake2b256 => {
                let mut hasher = multihash_codetable::Blake2b256::default();
                self.wrap(hash(&mut hasher, bytes)?)?
            }
            Self::Blake2b512 => {
                let mut hasher = multihash_codetable::Blake2b512::default();
                self.wrap(hash(&mut hasher, bytes)?)?
            }
            Self::Blake2s128 => {
                let mut hasher = multihash_codetable::Blake2s128::default();
                self.wrap(hash(&mut hasher, bytes)?)?
            }
            Self::Blake2s256 => {
                let mut hasher = multihash_codetable::Blake2s256::default();
                self.wrap(hash(&mut hasher, bytes)?)?
            }
            Self::Blake3_256 => {
                let mut hasher = multihash_codetable::Blake3_256::default();
                self.wrap(hash(&mut hasher, bytes)?)?
            }
            Self::Ripemd160 => {
                let mut hasher = multihash_codetable::Ripemd160::default();
                self.wrap(hash(&mut hasher, bytes)?)?
            }
            Self::Ripemd256 => {
                let mut hasher = multihash_codetable::Ripemd256::default();
                self.wrap(hash(&mut hasher, bytes)?)?
            }
            Self::Ripemd320 => {
                let mut hasher = multihash_codetable::Ripemd320::default();
                self.wrap(hash(&mut hasher, bytes)?)?
            }
            _ => {
                anyhow::bail!("`digest_byte_stream` is unimplemented for {self:?}");
            }
        })
    }
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

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;
    use crate::utils::rand::forest_rng;
    use rand::RngCore as _;

    #[test]
    fn test_digest_byte_stream() {
        use MultihashCode::*;

        for len in [0, 1, 100, 1024, 10000] {
            let mut bytes = vec![0; len];
            forest_rng().fill_bytes(&mut bytes);
            let mut cursor = Cursor::new(bytes.clone());
            for code in [
                Sha2_256, Sha2_512, Sha3_224, Sha3_256, Sha3_384, Sha3_512, Keccak224, Keccak256,
                Keccak384, Keccak512, Blake2b256, Blake2b512, Blake2s128, Blake2s256, Blake3_256,
                Ripemd160, Ripemd256, Ripemd320,
            ] {
                cursor.set_position(0);
                let mh1 = code.digest(&bytes);
                let mh2 = code.digest_byte_stream(&mut cursor).unwrap();
                assert_eq!(mh1, mh2);
            }
        }
    }
}
