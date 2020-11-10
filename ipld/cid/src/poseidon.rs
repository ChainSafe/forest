// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use core::convert::TryFrom;
use generic_array::GenericArray;
use multihash::{Digest, Size};

/// Multihash digest.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct PoseidonDigest<S: Size>(GenericArray<u8, S>);

impl<S: Size> Copy for PoseidonDigest<S> where S::ArrayType: Copy {}

impl<S: Size> AsRef<[u8]> for PoseidonDigest<S> {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl<S: Size> AsMut<[u8]> for PoseidonDigest<S> {
    fn as_mut(&mut self) -> &mut [u8] {
        &mut self.0
    }
}

impl<S: Size> From<GenericArray<u8, S>> for PoseidonDigest<S> {
    fn from(array: GenericArray<u8, S>) -> Self {
        Self(array)
    }
}

impl<S: Size> From<PoseidonDigest<S>> for GenericArray<u8, S> {
    fn from(digest: PoseidonDigest<S>) -> Self {
        digest.0
    }
}

/// Convert slice to `Digest`.
///
/// It errors when the length of the slice does not match the size of the `Digest`.
impl<S: Size> TryFrom<&[u8]> for PoseidonDigest<S> {
    type Error = multihash::Error;

    fn try_from(slice: &[u8]) -> Result<Self, Self::Error> {
        Self::wrap(slice)
    }
}

impl<S: Size> Digest<S> for PoseidonDigest<S> {}
