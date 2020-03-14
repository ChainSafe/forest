// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use address::Address;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use vm::MethodNum;

/// Cron actor state which holds entries to call during epoch tick
#[derive(Default)]
pub struct CronActorState {
    /// Entries is a set of actors (and corresponding methods) to call during EpochTick.
    pub entries: Vec<CronEntry>,
}

pub struct CronEntry {
    receiver: Address,
    method_num: MethodNum,
}

impl Serialize for CronActorState {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.entries.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for CronActorState {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let entries: Vec<CronEntry> = Deserialize::deserialize(deserializer)?;
        Ok(Self { entries })
    }
}

impl Serialize for CronEntry {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (&self.receiver, &self.method_num).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for CronEntry {
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
