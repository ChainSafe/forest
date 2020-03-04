// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod bitfield;
mod error;
mod hamt;
mod hash;
mod node;
mod pointer;

pub use self::error::Error;
pub use self::hamt::Hamt;
pub use self::hash::*;

use serde::{Deserialize, Serialize};

const MAX_ARRAY_WIDTH: usize = 3;

type HashedKey = [u8; 16];

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct KeyValuePair<K, V>(K, V);

impl<K, V> KeyValuePair<K, V> {
    pub fn key(&self) -> &K {
        &self.0
    }

    pub fn value(&self) -> &V {
        &self.1
    }
}

impl<K, V> KeyValuePair<K, V> {
    pub fn new(key: K, value: V) -> Self {
        KeyValuePair(key, value)
    }
}
