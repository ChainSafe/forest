// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use forest_beacon::{Beacon, ChainInfo, DrandBeacon, DrandConfig};
use serde::{Deserialize, Serialize};

async fn new_beacon() -> DrandBeacon {
    DrandBeacon::new(
        15904451751,
        25,
        // TODO this could maybe be referencing existing config
        &DrandConfig {
            server: "https://pl-us.incentinet.drand.sh",
            chain_info: ChainInfo {
                public_key: "922a2e93828ff83345bae533f5172669a26c02dc76d6bf59c80892e12ab1455c229211886f35bb56af6d5bea981024df"
                    .into(),
                ..Default::default()
            },
            network_type: forest_beacon::DrandNetwork::Incentinet
        },
    )
    .await
    .unwrap()
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BeaconEntryJson {
    round: u64,
    randomness: String,
    signature: String,
    previous_signature: String,
}

#[tokio::test]
async fn construct_drand_beacon() {
    new_beacon().await;
}

#[tokio::test]
async fn ask_and_verify_beacon_entry_fail() {
    let beacon = new_beacon().await;

    let e2 = beacon.entry(2).await.unwrap();
    let e3 = beacon.entry(3).await.unwrap();
    assert!(!beacon.verify_entry(&e2, &e3).await.unwrap());
}
