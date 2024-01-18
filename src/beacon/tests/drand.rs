// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::beacon::{Beacon, ChainInfo, DrandBeacon, DrandConfig, DrandNetwork};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;

fn new_beacon() -> DrandBeacon {
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
                    chain_info:  ChainInfo {
                        public_key: Cow::Borrowed("868f005eb8e6e4ca0a47c8a77ceaa5309a47978a7c71bc5cce96366b5d7a569937c529eeda66c7293784a9402801af31"),
                        period: 30,
                        genesis_time: 1595431050,
                        hash: Cow::Borrowed("8990e7a9aaed2ffed73dbd7092123d6f289930540d7651336225dc172e51b2ce"),
                        group_hash: Cow::Borrowed("176f93498eac9ca337150b46d21dd58673ea4e3581185f869672e59fa4cb390a"),
                    },
                    network_type: DrandNetwork::Mainnet,
                }
            )
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BeaconEntryJson {
    round: u64,
    randomness: String,
    signature: String,
    previous_signature: String,
}

#[test]
fn construct_drand_beacon() {
    new_beacon();
}

#[tokio::test]
async fn ask_and_verify_beacon_entry_success() {
    let beacon = new_beacon();

    let e2 = beacon.entry(2).await.unwrap();
    let e3 = beacon.entry(3).await.unwrap();
    assert!(beacon.verify_entry(&e3, &e2).unwrap());
}

#[tokio::test]
async fn ask_and_verify_beacon_entry_fail() {
    let beacon = new_beacon();

    let e2 = beacon.entry(2).await.unwrap();
    let e3 = beacon.entry(3).await.unwrap();
    assert!(!beacon.verify_entry(&e2, &e3).unwrap());
}
