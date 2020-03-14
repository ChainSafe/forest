// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use address::Address;
use serde::{Deserialize, Serialize};
use vm::MethodNum;

/// CronActorState has no internal state
// TODO implement tuple serialize/deserialize
#[derive(Default, Serialize, Deserialize)]
pub struct CronActorState {
    /// Entries is a set of actors (and corresponding methods) to call during EpochTick.
    /// This can be done a bunch of ways. We do it this way here to make it easy to add
    /// a handler to Cron elsewhere in the spec code. How to do this is implementation
    /// specific.
    entries: Vec<CronEntry>,
}

// TODO implement tuple serialize/deserialize
#[derive(Clone, Serialize, Deserialize)]
pub struct CronEntry {
    to_addr: Address,
    method_num: MethodNum,
}
