// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::Hash;
use serde::{Deserialize, Serialize};
use std::borrow::Borrow;
use std::hash::Hasher;
use std::ops::Deref;

/// Key type to be used to serialize as byte string instead of a `u8` array.
/// This type is used as a default for the `Hamt` as this is the only allowed type
/// with the go implementation.
#[derive(Eq, PartialOrd, Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(transparent)]
pub struct BytesKey(#[serde(with = "serde_bytes")] pub Vec<u8>);

impl Hash for BytesKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write(&self.0);
    }
}

impl Borrow<[u8]> for BytesKey {
    fn borrow(&self) -> &[u8] {
        &self.0
    }
}

impl Borrow<Vec<u8>> for BytesKey {
    fn borrow(&self) -> &Vec<u8> {
        &self.0
    }
}

impl Deref for BytesKey {
    type Target = Vec<u8>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<Vec<u8>> for BytesKey {
    fn from(bz: Vec<u8>) -> Self {
        BytesKey(bz)
    }
}

impl From<&[u8]> for BytesKey {
    fn from(s: &[u8]) -> Self {
        Self(s.to_vec())
    }
}

impl From<&str> for BytesKey {
    fn from(s: &str) -> Self {
        Self::from(s.as_bytes())
    }
}
