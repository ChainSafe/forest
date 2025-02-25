// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::{
    beacon::{Beacon, ChainInfo, DrandBeacon, DrandConfig, DrandNetwork},
    shim::version::NetworkVersion,
};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;

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

fn new_beacon_quicknet() -> DrandBeacon {
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

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BeaconEntryJson {
    round: u64,
    randomness: String,
    signature: String,
    previous_signature: Option<String>,
}

#[test]
fn construct_drand_beacon_mainnet() {
    new_beacon_mainnet();
}

#[test]
fn construct_drand_beacon_quicknet() {
    new_beacon_quicknet();
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

// https://github.com/filecoin-project/FIPs/pull/914/files#diff-fa537e813e7b41bd21980a06cf452f13e1b40e8a74f47a9f4bc4dd47c1df43b0L76
#[test]
fn test_max_beacon_round_for_epoch_quicknet() {
    let beacon = new_beacon_quicknet();
    let round = beacon.max_beacon_round_for_epoch(NetworkVersion::V21, 3547000);
    assert_eq!(
        round,
        ((1598306400 + 3547000 * 30) - 1692803367 - 30) / 3 + 1
    );
}
