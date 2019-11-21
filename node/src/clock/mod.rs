extern crate chrono;

use chrono::{Utc, DateTime, SecondsFormat, NaiveDateTime};
use std::time::Duration;

const ISO_FORMAT: &str = "%FT%X.%.9F";
const EPOCH_DURATION: i8 = 15;

/// The `Clock` trait defines must have functionality for filecoin clocks.
trait Clock {
    fn new(genesis_time: i64) -> ChainEpochClock;
    fn get_time(&self) -> DateTime<Utc>;
}

/// ChainEpochClock is used by the system node to assume weak clock synchrony amognst the other
/// systems.
struct ChainEpochClock {
    // Chain start time in ISO nano timestamp
    genesis_time: DateTime<Utc>
}

impl Clock for ChainEpochClock {
    /// Returns a ChainEpochClock based on the given genesis_time
    ///
    /// # Arguments
    ///
    /// * `genesis_time` - An i64 representing a unix timestamp
    fn new(genesis_time: i64) -> ChainEpochClock {
        // Convert unix timestamp
        let native_date_time = NaiveDateTime::from_timestamp(genesis_time, 0);

        // Convert to DateTime
        let date_time = DateTime::<Utc>::from_utc(native_date_time, Utc);

        // Use nanoseconds
        date_time.to_rfc3339_opts(SecondsFormat::Nanos, true);
        
        ChainEpochClock {
            genesis_time: date_time
        }        
    }
    
    /// Returns the genesis time as a DateTime<Utc>
    fn get_time(&self) -> DateTime<Utc> {
        self.genesis_time
    }

    fn epoch_at_time(&self, time: DateTime<Utc>) {
        let difference = time.signed_duration_since(self.genesis_time);
        // TODO Finish this based on spec
    }
}
