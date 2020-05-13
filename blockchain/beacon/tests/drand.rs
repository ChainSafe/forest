// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use beacon::{Beacon, DistPublic, DrandBeacon};

async fn new_beacon() -> DrandBeacon {
    // Current public parameters, subject to change.
    let coeffs = [
        hex::decode("82c279cce744450e68de98ee08f9698a01dd38f8e3be3c53f2b840fb9d09ad62a0b6b87981e179e1b14bc9a2d284c985").unwrap(),
        hex::decode("82d51308ad346c686f81b8094551597d7b963295cbf313401a93df9baf52d5ae98a87745bee70839a4d6e65c342bd15b").unwrap(),
        hex::decode("94eebfd53f4ba6a3b8304236400a12e73885e5a781509a5c8d41d2e8b476923d8ea6052649b3c17282f596217f96c5de").unwrap(),
        hex::decode("8dc4231e42b4edf39e86ef1579401692480647918275da767d3e558c520d6375ad953530610fd27daf110187877a65d0").unwrap(),
    ];
    let dist_pub = DistPublic {
        coefficients: coeffs,
    };
    DrandBeacon::new(dist_pub, 1, 25).await.unwrap()
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
    assert!(beacon.verify_entry(&e3, &e2).unwrap());
}

#[ignore]
#[async_std::test]
async fn ask_and_verify_beacon_entry_fail() {
    let beacon = new_beacon().await;

    let e2 = beacon.entry(2).await.unwrap();
    let e3 = beacon.entry(3).await.unwrap();
    assert!(!beacon.verify_entry(&e2, &e3).unwrap());
}
