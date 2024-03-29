// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::blocks::Tipset;
use fvm_ipld_blockstore::Blockstore;
use num::BigInt;
use std::sync::Arc;

pub type Weight = BigInt;

/// The `Scale` trait abstracts away the logic of assigning a weight to a chain,
/// which can be consensus specific. For example it can depend on the stake and
/// power of validators, or it can be as simple as the height of the blocks in
/// a `Nakamoto` style consensus.
pub trait Scale {
    /// Calculate the weight of a tipset.
    fn weight<DB>(db: &Arc<DB>, ts: &Tipset) -> Result<Weight, anyhow::Error>
    where
        DB: Blockstore;
}
