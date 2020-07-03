// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use chrono::{DateTime, NaiveDateTime, SecondsFormat, Utc};
use std::num::TryFromIntError;

const _ISO_FORMAT: &str = "%FT%X.%.9F";
const EPOCH_DURATION: i32 = 15;

pub type ChainEpoch = i64;
pub const EPOCH_UNDEFINED: ChainEpoch = -1;

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
        Ok(epochs)
    }
}
