// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use address::Address;
use encoding::{tuple::*, Cbor};
use serde::{Deserialize, Serialize};
use vm::MethodNum;

/// Cron actor state which holds entries to call during epoch tick
#[derive(Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct State {
    /// Entries is a set of actors (and corresponding methods) to call during EpochTick.
    pub entries: Vec<Entry>,
}

#[derive(Clone, PartialEq, Debug, Serialize_tuple, Deserialize_tuple)]
pub struct Entry {
    pub receiver: Address,
    pub method_num: MethodNum,
}

impl Cbor for State {}
