// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod builtin;
mod code;

pub use self::builtin::*;
pub use self::code::*;
use cid::Cid;
use encoding::{de, ser, Cbor};
use num_bigint::BigUint;
use serde::{Deserialize, Serialize};

/// Identifier for Actors, includes builtin and initialized actors
#[derive(PartialEq, Eq, Copy, Clone, Debug, Default, Serialize, Deserialize)]
pub struct ActorID(u64);

// TODO verify format or implement custom serialize/deserialize function (if necessary):
// https://github.com/ChainSafe/forest/issues/143

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

// TODO implement Actor for builtin actors on finished spec
/// Actor trait which defines the common functionality of system Actors
pub trait Actor {
    /// Returns Actor Cid
    fn cid(&self) -> &Cid;
    /// Actor public key, if it exists
    fn public_key(&self) -> Vec<u8>;
}

impl ser::Serialize for ActorState {
    fn serialize<S>(&self, s: S) -> Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        (
            self.code_id.clone(),
            self.state.clone(),
            self.sequence,
            self.balance.clone(),
        )
            .serialize(s)
    }
}

impl<'de> de::Deserialize<'de> for ActorState {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        let (code_id, state, sequence, balance) = Deserialize::deserialize(deserializer)?;
        Ok(Self {
            code_id,
            state,
            sequence,
            balance,
        })
    }
}
