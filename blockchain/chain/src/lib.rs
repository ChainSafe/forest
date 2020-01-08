// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

#![allow(dead_code)]

mod store;

pub use self::store::*;

/// Chain
struct Chain {
    // TODO add Message Store
    // TODO add State Reader
    _store: ChainStore,
}
