// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::beacon::{Beacon, ChainInfo, DrandBeacon, DrandConfig};
use serde::{Deserialize, Serialize};

fn new_beacon() -> DrandBeacon {
    // https://pl-us.incentinet.drand.sh/info
    DrandBeacon::new(
        15904451751,
        25,
        &DrandConfig {
            servers: vec!["https://pl-us.incentinet.drand.sh"],
            chain_info: ChainInfo {
                public_key: "8d4dc143b2128e18b4cdace6e5abece8012bfeca48551a008a69a1bbc88b71d37da840d2c8b028170f0a8704c90c1617"
                    .into(),
                period: 30,
                genesis_time: 1698856390,
                hash: "f11df9e56edb49c6b049cd73a68214be4e879688fdd696f96f0750ad377f9be4".into(),
                group_hash: "36ab1415e2967a7571f70f88cbf733eb77ef1a3ed34173ecc5e7bac924aeb17f".into(),
            },
            network_type: crate::beacon::DrandNetwork::Incentinet,
        },
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
