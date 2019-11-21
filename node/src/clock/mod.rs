extern crate chrono;

use chrono::{Utc, DateTime, SecondsFormat, NaiveDateTime};
use std::time::Duration;

const ISO_FORMAT: &str = "%FT%X.%.9F";
const EPOCH_DURATION: i8 = 15;

trait Clock {
    fn new(genesis_time: i64) -> ChainEpochClock;
    fn get_time(&self) -> DateTime<Utc>;
    fn epoch_at_time(&self, time: DateTime<Utc>);
}

struct ChainEpochClock {
    // Chain start time in ISO nano timestamp
    genesis_time: DateTime<Utc>
}

impl Clock for ChainEpochClock {
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

    fn get_time(&self) -> DateTime<Utc> {
        self.genesis_time
    }

    fn epoch_at_time(&self, time: DateTime<Utc>) {
        let difference = time.signed_duration_since(self.genesis_time);
        // TODO Finish this based on spec
    }
}