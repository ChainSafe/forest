// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use address::Address;
use encoding::Cbor;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use vm::MethodNum;

/// Cron actor state which holds entries to call during epoch tick
#[derive(Default)]
pub struct State {
    /// Entries is a set of actors (and corresponding methods) to call during EpochTick.
    pub entries: Vec<Entry>,
}

#[derive(Clone, PartialEq, Debug)]
pub struct Entry {
    pub receiver: Address,
    pub method_num: MethodNum,
}

impl Serialize for State {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.entries.serialize(serializer)
    }
}

impl Cbor for State {}

impl<'de> Deserialize<'de> for State {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let entries: Vec<Entry> = Deserialize::deserialize(deserializer)?;
        Ok(Self { entries })
    }
}

impl Serialize for Entry {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (&self.receiver, &self.method_num).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Entry {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (receiver, method_num) = Deserialize::deserialize(deserializer)?;
        Ok(Self {
            receiver,
            method_num,
        })
    }
}
