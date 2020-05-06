// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

/// Represents either a key in a map or an index in a list.
#[derive(Clone, Debug)]
pub enum PathSegment {
    /// Key in a map
    String(String),
    /// Index in a list
    Int(usize),
}

impl From<usize> for PathSegment {
    fn from(i: usize) -> Self {
        Self::Int(i)
    }
}

impl From<String> for PathSegment {
    fn from(s: String) -> Self {
        Self::String(s)
    }
}
