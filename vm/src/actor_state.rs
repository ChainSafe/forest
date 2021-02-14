// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::TokenAmount;
use cid::Cid;
use encoding::tuple::*;
use num_bigint::bigint_ser;

/// State of all actor implementations.
#[derive(PartialEq, Eq, Clone, Debug, Serialize_tuple, Deserialize_tuple)]
pub struct ActorState {
    /// Link to code for the actor.
    pub code: Cid,
    /// Link to the state of the actor.
    pub state: Cid,
    /// Sequence of the actor.
    pub sequence: u64,
    /// Tokens available to the actor.
    #[serde(with = "bigint_ser")]
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

#[cfg(feature = "json")]
pub mod json {
    use super::*;
    use crate::TokenAmount;
    use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
    use std::str::FromStr;

    /// Wrapper for serializing and deserializing a SignedMessage from JSON.
    #[derive(Deserialize, Serialize)]
    #[serde(transparent)]
    pub struct ActorStateJson(#[serde(with = "self")] pub ActorState);

    /// Wrapper for serializing a SignedMessage reference to JSON.
    #[derive(Serialize)]
    #[serde(transparent)]
    pub struct ActorStateJsonRef<'a>(#[serde(with = "self")] pub &'a ActorState);

    impl From<ActorStateJson> for ActorState {
        fn from(wrapper: ActorStateJson) -> Self {
            wrapper.0
        }
    }

    pub fn serialize<S>(m: &ActorState, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        #[derive(Serialize)]
        #[serde(rename_all = "PascalCase")]
        struct ActorStateSer<'a> {
            #[serde(with = "cid::json")]
            pub code: &'a Cid,
            #[serde(rename = "Head", with = "cid::json")]
            pub state: &'a Cid,
            #[serde(rename = "Nonce")]
            pub sequence: u64,
            pub balance: String,
        }
        ActorStateSer {
            code: &m.code,
            state: &m.state,
            sequence: m.sequence,
            balance: m.balance.to_str_radix(10),
        }
        .serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<ActorState, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(rename_all = "PascalCase")]
        struct ActorStateDe {
            #[serde(with = "cid::json")]
            pub code: Cid,
            #[serde(rename = "Head", with = "cid::json")]
            pub state: Cid,
            #[serde(rename = "Nonce")]
            pub sequence: u64,
            pub balance: String,
        }
        let ActorStateDe {
            code,
            state,
            sequence,
            balance,
        } = Deserialize::deserialize(deserializer)?;
        Ok(ActorState {
            code,
            state,
            sequence,
            balance: TokenAmount::from_str(&balance).map_err(de::Error::custom)?,
        })
    }
}
