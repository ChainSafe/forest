// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use chrono::{DateTime, NaiveDateTime, SecondsFormat, Utc};
use encoding::{
    de::{self, Deserializer},
    ser::{self, Serializer},
};
use std::convert::TryInto;
use std::num::TryFromIntError;
use std::ops::Sub;

const _ISO_FORMAT: &str = "%FT%X.%.9F";
const EPOCH_DURATION: i32 = 15;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
/// An epoch represents a single valid state in the blockchain
pub struct ChainEpoch(u64);

impl ser::Serialize for ChainEpoch {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.0.serialize(serializer)
    }
}

impl<'de> de::Deserialize<'de> for ChainEpoch {
    fn deserialize<D>(deserializer: D) -> Result<Self, <D as Deserializer<'de>>::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(ChainEpoch(de::Deserialize::deserialize(deserializer)?))
    }
}

// TODO verify format or implement custom serialize/deserialize function (if necessary):
// https://github.com/ChainSafe/forest/issues/143

/// ChainEpochClock is used by the system node to assume weak clock synchrony amongst the other
/// systems.
pub struct ChainEpochClock {
    // Chain start time in ISO nano timestamp
    genesis_time: DateTime<Utc>,
}

impl ChainEpochClock {
    /// Returns a ChainEpochClock based on the given genesis_time (unix timestamp)
    pub fn new(genesis_time: i64) -> ChainEpochClock {
        // Convert unix timestamp
        let native_date_time = NaiveDateTime::from_timestamp(genesis_time, 0);

        // Convert to DateTime
        let date_time = DateTime::<Utc>::from_utc(native_date_time, Utc);

        // Use nanoseconds
        date_time.to_rfc3339_opts(SecondsFormat::Nanos, true);

        ChainEpochClock {
            genesis_time: date_time,
        }
    }

    /// Returns the genesis time as a `DateTime<Utc>`
    pub fn get_genesis_time(&self) -> DateTime<Utc> {
        self.genesis_time
    }

    /// Returns the epoch at a given time
    pub fn epoch_at_time(&self, time: &DateTime<Utc>) -> Result<ChainEpoch, TryFromIntError> {
        let difference = time.signed_duration_since(self.genesis_time);
        let epochs = (difference / EPOCH_DURATION)
            .num_nanoseconds()
            .expect("Epoch_at_time failed");
        Ok(ChainEpoch(epochs.try_into()?))
    }
}

impl Sub for ChainEpoch {
    type Output = ChainEpoch;

    fn sub(self, other: ChainEpoch) -> ChainEpoch {
        ChainEpoch {
            0: self.0 - other.0,
        }
    }
}

impl ChainEpoch {
    /// Returns ChainEpoch based on the given unix timestamp
    pub fn new(timestamp: i64) -> Result<ChainEpoch, TryFromIntError> {
        Ok(ChainEpoch(timestamp.try_into()?))
    }
    // Returns chain epoch
    pub fn chain_epoch(&self) -> &u64 {
        &self.0
    }
}
