// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use chrono::{DateTime, NaiveDateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use std::convert::TryInto;
use std::num::TryFromIntError;
use std::ops::{Add, Sub};

const _ISO_FORMAT: &str = "%FT%X.%.9F";
const EPOCH_DURATION: i32 = 15;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default, PartialOrd, Serialize, Deserialize)]
/// An epoch represents a single valid state in the blockchain
pub struct ChainEpoch(pub u64);

impl From<u64> for ChainEpoch {
    fn from(num: u64) -> ChainEpoch {
        ChainEpoch(num)
    }
}

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

impl Add for ChainEpoch {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self(self.0 + other.0)
    }
}

// TODO revisit usage of the Sub impls, these will panic (checked sub or floored sub would be safer)
impl Sub for ChainEpoch {
    type Output = ChainEpoch;

    fn sub(self, other: ChainEpoch) -> ChainEpoch {
        ChainEpoch(self.0 - other.0)
    }
}

impl Sub for &ChainEpoch {
    type Output = ChainEpoch;

    fn sub(self, other: &ChainEpoch) -> ChainEpoch {
        ChainEpoch(self.0 - other.0)
    }
}

impl From<ChainEpoch> for u64 {
    fn from(ce: ChainEpoch) -> u64 {
        ce.0
    }
}

impl ChainEpoch {
    /// Returns ChainEpoch based on the given unix timestamp
    pub fn new(timestamp: i64) -> Result<ChainEpoch, TryFromIntError> {
        Ok(ChainEpoch(timestamp.try_into()?))
    }
    // Returns chain epoch as u64
    pub fn as_u64(&self) -> &u64 {
        &self.0
    }
}
