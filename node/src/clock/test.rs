#![cfg(all(test))]
use crate::clock::ChainEpochClock;

#[test]
fn create_chain_epoch_clock() {
    let utc_timestamp = 1574286946904;
    let clock = ChainEpochClock::new(utc_timestamp);
    assert_eq!(clock.get_time().timestamp(), utc_timestamp);
}
