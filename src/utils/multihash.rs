// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//!
//! This module back-fills the Identify hasher and code that was removed in `multihash` crate.
//! See <https://github.com/multiformats/rust-multihash/blob/master/CHANGELOG.md#-breaking-changes>
//! and <https://github.com/multiformats/rust-multihash/pull/289>
//!

pub mod prelude {
    pub use super::MultihashCodeLegacy;
    pub use multihash_codetable::{Multihash, Code as MultihashCode, MultihashDigest as _};
}

use multihash_derive::MultihashDigest;

#[derive(Clone, Copy, Debug, Eq, MultihashDigest, PartialEq)]
#[mh(alloc_size = 64)]
pub enum MultihashCodeLegacy {
    #[mh(code = 0x0, hasher = IdentityHasher::<64>)]
    Identity,
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
