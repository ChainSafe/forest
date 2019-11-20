extern crate chrono;

use chrono::{Utc}

trait Clock {
    func now_utc() -> &str;
    func now_utc_unix() -> &str;
    func now_utc_unix_nano() -> &str;
}

struct ChainEpochClock {
    // Chain start time in ISO nano timestamp
    genesis_time String
}

impl Clock for ChainEpochClock {
    fn now_utc() -> &str {
        let now = Utc.now();
        println!("{:?}", now);
        return now;
    }
}

#[test]
fn check_utc() {
    let clock: ChainEpochClock {
        genesis_time: "12312321"
    }
    let now = clock.now_utc();
    println!("{:?}", now);
    assert!(true);
}