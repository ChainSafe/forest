// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::TokenAmount;
use cid::Cid;
use encoding::tuple::*;
use num_bigint::biguint_ser;

/// Identifier for Actors, includes builtin and initialized actors
pub type ActorID = u64;

/// State of all actor implementations
#[derive(PartialEq, Eq, Clone, Debug, Serialize_tuple, Deserialize_tuple)]
pub struct ActorState {
    pub code: Cid,
    pub state: Cid,
    pub sequence: u64,
    #[serde(with = "biguint_ser")]
    pub balance: TokenAmount,
}

impl ActorState {
    /// Constructor for actor state
    pub fn new(code: Cid, state: Cid, balance: TokenAmount, sequence: u64) -> Self {
        Self {
            code,
            state,
            balance,
            sequence,
        }
    }
    /// Safely deducts funds from an Actor
    pub fn deduct_funds(&mut self, amt: &TokenAmount) -> Result<(), String> {
        if &self.balance < amt {
            return Err("Not enough funds".to_owned());
        }
        self.balance -= amt;

        Ok(())
    }
    /// Deposits funds to an Actor
    pub fn deposit_funds(&mut self, amt: &TokenAmount) {
        self.balance += amt;
    }
}
