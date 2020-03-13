// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::CodeID;
use cid::Cid;
use encoding::Cbor;
use num_bigint::BigUint;
use serde::{Deserialize, Serialize};

/// Identifier for Actors, includes builtin and initialized actors
#[derive(PartialEq, Eq, Copy, Clone, Debug, Default, Serialize, Deserialize)]
pub struct ActorID(pub u64);

impl Cbor for ActorID {}

/// State of all actor implementations
#[derive(PartialEq, Eq, Clone, Debug)]
pub struct ActorState {
    pub code_id: CodeID,
    pub state: Cid,
    pub balance: BigUint,
    pub sequence: u64,
}

impl ActorState {
    /// Constructor for actor state
    pub fn new(code_id: CodeID, state: Cid, balance: BigUint, sequence: u64) -> Self {
        Self {
            code_id,
            state,
            balance,
            sequence,
        }
    }
}
