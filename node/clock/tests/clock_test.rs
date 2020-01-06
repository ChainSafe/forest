// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

use clock::ChainEpochClock;

#[test]
fn create_chain_epoch_clock() {
    let utc_timestamp = 1_574_286_946_904;
    let clock = ChainEpochClock::new(utc_timestamp);
    assert_eq!(clock.get_genesis_time().timestamp(), utc_timestamp);
}
