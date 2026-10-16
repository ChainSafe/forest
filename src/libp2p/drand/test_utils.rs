// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::sync::Arc;

use libp2p::gossipsub::MessageAcceptance;

use crate::beacon::{
    Beacon as _, BeaconPoint, BeaconSchedule, tests::fake_drand_signer::FakeDrandSigner,
};

/// On the test chain, every epoch draws from a round ending in 2.
pub(in crate::libp2p) const EPOCH_ROUND: u64 = 12;

pub(in crate::libp2p) fn fake_drand_schedule() -> (FakeDrandSigner, Arc<BeaconSchedule>) {
    let drand = FakeDrandSigner::new();
    let beacon = drand.beacon();
    (
        drand,
        Arc::new(BeaconSchedule(vec![BeaconPoint::new(0, beacon)])),
    )
}

/// Unix time right after `round` is produced by the fake `drand` chain.
pub(in crate::libp2p) fn time_of(schedule: &BeaconSchedule, round: u64) -> u64 {
    schedule
        .unchained_beacon()
        .and_then(|beacon| beacon.beacon_round_timestamp(round))
        .expect("fake drand beacon has round timestamps")
        + 1
}

pub(in crate::libp2p) fn is_cached(schedule: &BeaconSchedule, round: u64) -> bool {
    schedule
        .unchained_beacon()
        .is_some_and(|beacon| beacon.is_cached(round))
}

// `MessageAcceptance` doesn't implement `PartialEq`.
pub(in crate::libp2p) fn assert_verdict(actual: &MessageAcceptance, expected: MessageAcceptance) {
    assert_eq!(
        std::mem::discriminant(actual),
        std::mem::discriminant(&expected),
        "verdict {actual:?}, expected {expected:?}"
    );
}
