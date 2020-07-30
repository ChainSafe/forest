// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use beacon::{Beacon, DrandBeacon, DrandPublic};
use serde::{Deserialize, Serialize};

async fn new_beacon() -> DrandBeacon {
    // Current public parameters, subject to change.
    let coeffs =
        hex::decode("922a2e93828ff83345bae533f5172669a26c02dc76d6bf59c80892e12ab1455c229211886f35bb56af6d5bea981024df").unwrap();
    let dist_pub = DrandPublic {
        coefficient: coeffs,
    };
    DrandBeacon::new(dist_pub, 15904451751, 25).await.unwrap()
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BeaconEntryJson {
    round: u64,
    randomness: String,
    signature: String,
    previous_signature: String,
}

#[ignore]
#[async_std::test]
async fn construct_drand_beacon() {
    new_beacon().await;
}

#[ignore]
#[async_std::test]
async fn ask_and_verify_beacon_entry() {
    let beacon = new_beacon().await;

    let e2 = beacon.entry(2).await.unwrap();
    let e3 = beacon.entry(3).await.unwrap();
    assert!(beacon.verify_entry(&e3, &e2).await.unwrap());
}

#[ignore]
#[async_std::test]
async fn ask_and_verify_beacon_entry_fail() {
    let beacon = new_beacon().await;

    let e2 = beacon.entry(2).await.unwrap();
    let e3 = beacon.entry(3).await.unwrap();
    assert!(!beacon.verify_entry(&e2, &e3).await.unwrap());
}
