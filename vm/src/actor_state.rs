// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use cid::Cid;
use encoding::Cbor;
use num_bigint::{biguint_ser, BigUint};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::ops::AddAssign;

/// Identifier for Actors, includes builtin and initialized actors
#[derive(PartialEq, Eq, Copy, Clone, Debug, Serialize, Deserialize, Default)]
pub struct ActorID(pub u64);

impl AddAssign<u64> for ActorID {
    fn add_assign(&mut self, other: u64) {
        self.0 += other
    }
}

impl Cbor for ActorID {}

/// State of all actor implementations
#[derive(PartialEq, Eq, Clone, Debug)]
pub struct ActorState {
    pub code: Cid,
    pub state: Cid,
    pub balance: BigUint,
    pub sequence: u64,
}

impl Serialize for ActorState {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        #[derive(Serialize)]
        struct TupleActorState<'a>(
            &'a Cid,
            &'a Cid,
            &'a u64,
            #[serde(with = "biguint_ser")] &'a BigUint,
        );
        TupleActorState(&self.code, &self.state, &self.sequence, &self.balance)
            .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ActorState {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct TupleActorState(Cid, Cid, u64, #[serde(with = "biguint_ser")] BigUint);
        let TupleActorState(code, state, sequence, balance) =
            Deserialize::deserialize(deserializer)?;
        Ok(ActorState {
            code,
            state,
            sequence,
            balance,
        })
    }
}

impl ActorState {
    /// Constructor for actor state
    pub fn new(code: Cid, state: Cid, balance: BigUint, sequence: u64) -> Self {
        Self {
            code,
            state,
            balance,
            sequence,
        }
    }
}
