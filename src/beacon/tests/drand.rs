// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use itertools::Itertools;

use crate::beacon::drand::beacon_round_wait;
use crate::{
    beacon::mock_beacon::MockBeacon,
    beacon::{
        Beacon, BeaconEntry, BeaconPoint, BeaconSchedule, ChainInfo, DrandBeacon, DrandConfig,
        DrandNetwork,
    },
    shim::{clock::ChainEpoch, version::NetworkVersion},
};
use quickcheck_macros::quickcheck;
use rstest::rstest;
use std::borrow::Cow;
use std::sync::LazyLock;
use std::time::Duration;

fn new_beacon_mainnet() -> DrandBeacon {
    DrandBeacon::new(
        1598306400,
        30,
        &DrandConfig {
            // https://drand.love/developer/http-api/#public-endpoints
            servers: vec![
                "https://api.drand.sh".try_into().unwrap(),
                "https://api2.drand.sh".try_into().unwrap(),
                "https://api3.drand.sh".try_into().unwrap(),
                "https://drand.cloudflare.com".try_into().unwrap(),
                "https://api.drand.secureweb3.com:6875".try_into().unwrap(),
            ],
            // https://api.drand.sh/8990e7a9aaed2ffed73dbd7092123d6f289930540d7651336225dc172e51b2ce/info
            chain_info: ChainInfo {
                public_key: Cow::Borrowed(
                    "868f005eb8e6e4ca0a47c8a77ceaa5309a47978a7c71bc5cce96366b5d7a569937c529eeda66c7293784a9402801af31",
                ),
                period: 30,
                genesis_time: 1595431050,
                hash: Cow::Borrowed(
                    "8990e7a9aaed2ffed73dbd7092123d6f289930540d7651336225dc172e51b2ce",
                ),
                group_hash: Cow::Borrowed(
                    "176f93498eac9ca337150b46d21dd58673ea4e3581185f869672e59fa4cb390a",
                ),
            },
            network_type: DrandNetwork::Mainnet,
        },
    )
}

pub fn new_beacon_quicknet() -> DrandBeacon {
    DrandBeacon::new(
        1598306400,
        30,
        &DrandConfig {
            // https://drand.love/developer/http-api/#public-endpoints
            servers: vec![
                "https://api.drand.sh".try_into().unwrap(),
                "https://api2.drand.sh".try_into().unwrap(),
                "https://api3.drand.sh".try_into().unwrap(),
                "https://drand.cloudflare.com".try_into().unwrap(),
                "https://api.drand.secureweb3.com:6875".try_into().unwrap(),
            ],
            // https://api.drand.sh/52db9ba70e0cc0f6eaf7803dd07447a1f5477735fd3f661792ba94600c84e971/info
            chain_info: ChainInfo {
                public_key: Cow::Borrowed(
                    "83cf0f2896adee7eb8b5f01fcad3912212c437e0073e911fb90022d3e760183c8c4b450b6a0a6c3ac6a5776a2d1064510d1fec758c921cc22b0e17e63aaf4bcb5ed66304de9cf809bd274ca73bab4af5a6e9c76a4bc09e76eae8991ef5ece45a",
                ),
                period: 3,
                genesis_time: 1692803367,
                hash: Cow::Borrowed(
                    "52db9ba70e0cc0f6eaf7803dd07447a1f5477735fd3f661792ba94600c84e971",
                ),
                group_hash: Cow::Borrowed(
                    "f477d5c89f21a17c863a7f937c6a6d15859414d2be09cd448d4279af331c5d3e",
                ),
            },
            network_type: DrandNetwork::Quicknet,
        },
    )
}

static MAINNET: LazyLock<DrandBeacon> = LazyLock::new(new_beacon_mainnet);
static QUICKNET: LazyLock<DrandBeacon> = LazyLock::new(new_beacon_quicknet);

#[test]
fn construct_drand_beacon_mainnet() {
    new_beacon_mainnet();
}

#[test]
fn construct_drand_beacon_quicknet() {
    new_beacon_quicknet();
}

#[test]
fn beacon_round_timestamp_quicknet() {
    // quicknet: genesis 1692803367, period 3s; round 1 is produced at genesis.
    let beacon = &*QUICKNET;
    assert_eq!(beacon.beacon_round_timestamp(1), Some(1_692_803_367));
    assert_eq!(beacon.beacon_round_timestamp(2), Some(1_692_803_370));
    assert_eq!(
        beacon.beacon_round_timestamp(100),
        Some(1_692_803_367 + 99 * 3)
    );
    // Round 0 saturates instead of underflowing.
    assert_eq!(beacon.beacon_round_timestamp(0), Some(1_692_803_367));
}

#[test]
fn beacon_round_timestamp_default_is_none() {
    // Non-drand beacons (e.g. the mock) fall back to the trait default, which signals
    // "no wait derivable" to callers.
    assert_eq!(MockBeacon::default().beacon_round_timestamp(42), None);
}

#[quickcheck]
fn beacon_round_timestamp_no_panic(round: u64) {
    // Must not overflow-panic for any round, including u64::MAX.
    let _ = QUICKNET.beacon_round_timestamp(round);
}

#[rstest]
// Round produced in the future: wait until then, plus the 1s publish buffer.
#[case(1_000, 900, Duration::from_secs(101))]
// `now` exactly at the round's production time: only the 1s buffer remains.
#[case(1_000, 1_000, Duration::from_secs(1))]
// Buffer already elapsed: no wait.
#[case(1_000, 1_001, Duration::ZERO)]
// Round long past: no wait.
#[case(1_000, 5_000, Duration::ZERO)]
// Negative clock is clamped to 0, never panics or under-waits.
#[case(1_000, -5, Duration::from_secs(1_001))]
fn beacon_round_wait_cases(#[case] round_ts: u64, #[case] now: i64, #[case] expected: Duration) {
    assert_eq!(beacon_round_wait(round_ts, now), expected);
}

#[quickcheck]
fn beacon_round_wait_no_panic(round_ts: u64, now: i64) {
    let _ = beacon_round_wait(round_ts, now);
}

#[tokio::test]
async fn ask_and_verify_mainnet_beacon_entry_success() {
    let beacon = new_beacon_mainnet();

    let e1 = beacon.entry(1).await.unwrap();
    let e2 = beacon.entry(2).await.unwrap();
    let e3 = beacon.entry(3).await.unwrap();
    assert!(beacon.verify_entries(&[e2, e3], &e1).unwrap());
}

// This is a regression test for cases when a block header contains
// duplicate beacon entries.
// For details, see <https://github.com/ChainSafe/forest/pull/4163>
#[tokio::test]
async fn ask_and_verify_mainnet_beacon_entry_success_issue_4163() {
    let beacon = new_beacon_mainnet();

    let e1 = beacon.entry(3907446).await.unwrap();
    let e2 = beacon.entry(3907447).await.unwrap();
    let e3 = beacon.entry(3907447).await.unwrap();
    assert!(beacon.verify_entries(&[e2, e3], &e1).unwrap());
}

#[tokio::test]
async fn ask_and_verify_mainnet_beacon_entry_fail() {
    let beacon = new_beacon_mainnet();

    let e1 = beacon.entry(1).await.unwrap();
    let e2 = beacon.entry(2).await.unwrap();
    let e3 = beacon.entry(3).await.unwrap();
    assert!(!beacon.verify_entries(&[e3, e2], &e1).unwrap());
}

#[tokio::test]
async fn ask_and_verify_quicknet_beacon_entry_success() {
    let beacon = new_beacon_quicknet();

    let e1 = beacon.entry(1).await.unwrap();
    let e2 = beacon.entry(2).await.unwrap();
    let e3 = beacon.entry(3).await.unwrap();
    assert!(beacon.verify_entries(&[e2, e3], &e1).unwrap());
}

#[tokio::test]
async fn ask_and_verify_quicknet_beacon_entry_success_2() {
    let beacon = new_beacon_quicknet();

    let e1 = beacon.entry(1).await.unwrap();
    let e2 = beacon.entry(2).await.unwrap();
    let e3 = beacon.entry(3).await.unwrap();
    assert!(beacon.verify_entries(&[e3, e2], &e1).unwrap());
}

#[quickcheck]
fn max_beacon_round_for_epoch_no_panic(fil_epoch: ChainEpoch) {
    for nv in [NetworkVersion::V15, NetworkVersion::V16] {
        let _ = QUICKNET.max_beacon_round_for_epoch(nv, fil_epoch);
    }
}

/// Expected rounds derived from FIP-0063 timings.
#[rstest]
#[case(0, 95844, 95845)]
#[case(1, 95845, 95846)]
#[case(100, 95944, 95945)]
fn max_beacon_round_for_epoch_mainnet(
    #[case] epoch: ChainEpoch,
    #[case] chained: u64,
    #[case] unchained: u64,
) {
    let round = |nv| MAINNET.max_beacon_round_for_epoch(nv, epoch).unwrap();
    assert_eq!(round(NetworkVersion::V15), chained);
    assert_eq!(round(NetworkVersion::V16), unchained);
}

#[rstest]
// Quicknet genesis postdates these epochs, so the first round stands in.
#[case(0, 1)]
#[case(3149899, 1)]
// First epoch at or after quicknet genesis, then the next: 10 drand rounds per 30s epoch.
#[case(3149900, 2)]
#[case(3149901, 12)]
// Also asserted against the live network by `beacon_entries_for_block_covers_null_rounds_quicknet`.
#[case(6216200, 30663002)]
// https://github.com/filecoin-project/FIPs/pull/914/files#diff-fa537e813e7b41bd21980a06cf452f13e1b40e8a74f47a9f4bc4dd47c1df43b0L76
#[case(3547000, 3971002)]
fn max_beacon_round_for_epoch_quicknet(#[case] epoch: ChainEpoch, #[case] expected: u64) {
    let round = QUICKNET
        .max_beacon_round_for_epoch(NetworkVersion::V22, epoch)
        .unwrap();
    assert_eq!(round, expected);
}

#[rstest]
#[case(i64::MIN)]
#[case(i64::MAX)]
fn max_beacon_round_for_epoch_rejects_out_of_range_epochs(#[case] epoch: ChainEpoch) {
    assert!(
        QUICKNET
            .max_beacon_round_for_epoch(NetworkVersion::V21, epoch)
            .is_err()
    );
}

/// `MockBeacon` is chained and serves entries locally, so the chained paths need no drand server.
#[tokio::test]
async fn beacon_entries_for_block_chained_walks_elapsed_rounds() {
    let schedule = BeaconSchedule(vec![BeaconPoint::new(0, MockBeacon::default())]);
    let prev = BeaconEntry::new(3, vec![]);

    let entries = schedule
        .beacon_entries_for_block(NetworkVersion::V15, 5, 3, &prev)
        .await
        .unwrap();

    assert_eq!(entries.iter().map(BeaconEntry::round).collect_vec(), [4, 5]);
}

#[tokio::test]
async fn beacon_entries_for_block_takes_two_entries_at_a_beacon_fork() {
    let schedule = BeaconSchedule(vec![
        BeaconPoint::new(0, MockBeacon::default()),
        BeaconPoint::new(10, MockBeacon::default()),
    ]);
    let prev = BeaconEntry::new(9, vec![]);

    let entries = schedule
        .beacon_entries_for_block(NetworkVersion::V15, 10, 9, &prev)
        .await
        .unwrap();

    assert_eq!(
        entries.iter().map(BeaconEntry::round).collect_vec(),
        [9, 10]
    );
}

#[tokio::test]
async fn beacon_entries_for_block_covers_null_rounds_quicknet() {
    // (parent epoch, its beacon round, block epoch, expected rounds)
    let cases = [
        // Null round at 6216199: entries for both 6216199 and 6216200.
        (6216198, 30662982, 6216200, vec![30662992, 30663002]),
        // No null round in between: only 6216200's entry.
        (6216199, 30662992, 6216200, vec![30663002]),
    ];

    let schedule = BeaconSchedule(vec![BeaconPoint::new(0, new_beacon_quicknet())]);

    for (prev_epoch, prev_epoch_round, epoch, expected_rounds) in cases {
        let (_, prev_beacon) = schedule.beacon_for_epoch(prev_epoch).unwrap();
        let prev_beacon_entry = prev_beacon.entry(prev_epoch_round).await.unwrap();

        let entries = schedule
            .beacon_entries_for_block(NetworkVersion::V22, epoch, prev_epoch, &prev_beacon_entry)
            .await
            .unwrap();

        let rounds = entries.iter().map(BeaconEntry::round).collect_vec();

        assert_eq!(
            rounds, expected_rounds,
            "epoch {epoch}, parent {prev_epoch}"
        );
    }
}