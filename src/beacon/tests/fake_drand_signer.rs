// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use quick_protobuf::{BytesReader, MessageRead as _, MessageWrite as _, Writer};

use super::drand::new_beacon_quicknet;
use crate::beacon::{
    Beacon, BeaconEntry, ChainInfo, DrandBeacon, DrandConfig, DrandNetwork,
    drand_pb::PublicRandResponse, signatures::sign_unchained,
};
use crate::networks::{QUICKNET_CHAIN_INFO, mainnet};
use crate::shim::clock::EPOCH_DURATION_SECONDS;
use crate::utils::encoding::hex;
use blstrs::{G2Projective, Scalar};
use group::{Curve, Group};

/// Signs any round of a `quicknet`-like chain with a made-up key, and encodes it as `drand`
/// gossips it, uses:
/// <https://github.com/drand/drand/blob/v2.1.7/crypto/schemes.go#L212-L240>.
pub struct FakeDrandSigner {
    secret: Scalar,
    config: DrandConfig<'static>,
}

impl FakeDrandSigner {
    pub fn new() -> Self {
        // Arbitrary: any non-zero scalar makes a valid key pair.
        let secret = Scalar::from(0xC0FFEEu64);
        let public = G2Projective::generator() * secret;
        let public_key = hex::encode(public.to_affine().to_compressed());
        // Real `quicknet` timing, so rounds map to epochs as on mainnet; the key and hashes
        // are made up.
        let quicknet = QUICKNET_CHAIN_INFO;
        Self {
            secret,
            config: DrandConfig {
                servers: vec![],
                chain_info: ChainInfo {
                    public_key: public_key.into(),
                    period: quicknet.period,
                    genesis_time: quicknet.genesis_time,
                    hash: "0011".repeat(16).into(),
                    group_hash: "00".repeat(32).into(),
                },
                network_type: DrandNetwork::Quicknet,
                // Would clash with the real quicknet config's collector.
                register_metrics: false,
            },
        }
    }

    pub fn entry(&self, round: u64) -> BeaconEntry {
        let signature = sign_unchained(self.secret, BeaconEntry::message_unchained(round).as_ref());
        BeaconEntry::new(round, signature.0.to_compressed().to_vec())
    }

    /// `round`, signed, as a gossiped `PublicRandResponse`.
    pub fn encode(&self, round: u64) -> Vec<u8> {
        Self::encode_raw(round, self.entry(round).signature())
    }

    /// A gossiped `PublicRandResponse` with any signature.
    pub fn encode_raw(round: u64, signature: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        PublicRandResponse {
            round,
            signature: signature.to_vec(),
        }
        .write_message(&mut Writer::new(&mut out))
        .unwrap();
        out
    }

    pub fn decode(payload: &[u8]) -> BeaconEntry {
        let response =
            PublicRandResponse::from_reader(&mut BytesReader::from_bytes(payload), payload)
                .unwrap();
        BeaconEntry::new(response.round, response.signature)
    }

    pub fn beacon(&self) -> DrandBeacon {
        DrandBeacon::new(
            mainnet::GENESIS_TIMESTAMP,
            EPOCH_DURATION_SECONDS as u64,
            &self.config,
        )
    }
}

// Two consecutive decoded `quicknet` rounds as drand's `api.drand.sh` relay gossips them:
// [
//   {
//     "round": 33233273,
//     "signature": "86665198ae4cc688d5612997d9a2d544ca83cf7ea72861038fad1992497fe95fd8d13c4ffd9474d55d79c46c8a95e737",
//     "randomness": "713a75c9c5b1c3aef49ba4d9f4e7261b99d3871ed3b430dfc882bcffe2835687"
//   },
//   {
//     "round": 33233274,
//     "signature": "a5081cfe5ef2c4d72288af2d931e1189adc1cf6eedddbd838a79aa742b44b0eecea6fb608c1cdd35b87551d4a843e5d7",
//     "randomness": "525fb5ec54032812a9091770c20f101706c93e3c8334edec80a2eadf43930f75"
//   }
// ]
#[cfg(test)]
const RELAY_PAYLOADS: [&str; 2] = [
    "08f9b2ec0f123086665198ae4cc688d5612997d9a2d544ca83cf7ea72861038fad1992497fe95fd8d13c4ffd9474d55d79c46c8a95e7372220713a75c9c5b1c3aef49ba4d9f4e7261b99d3871ed3b430dfc882bcffe2835687",
    "08fab2ec0f1230a5081cfe5ef2c4d72288af2d931e1189adc1cf6eedddbd838a79aa742b44b0eecea6fb608c1cdd35b87551d4a843e5d72220525fb5ec54032812a9091770c20f101706c93e3c8334edec80a2eadf43930f75",
];

#[rstest::rstest]
#[case::signed_rounds(
    true,
    |d: &FakeDrandSigner| (1..=5).map(|round| d.encode(round)).collect(), 
    true)
]
#[case::signed_for_another_round(
    true,
    |d: &FakeDrandSigner| vec![FakeDrandSigner::encode_raw(2, d.entry(3).signature())],
    false
)]
#[case::relay_rounds(
    false,
    |_: &FakeDrandSigner| RELAY_PAYLOADS.iter().map(|payload| hex::decode(payload).unwrap()).collect(),
    true
)]
fn entries_verify(
    #[case] fake: bool,
    #[case] payloads: fn(&FakeDrandSigner) -> Vec<Vec<u8>>,
    #[case] valid: bool,
) {
    let d = FakeDrandSigner::new();
    let beacon = if fake {
        d.beacon()
    } else {
        new_beacon_quicknet()
    };

    let entries: Vec<_> = payloads(&d)
        .iter()
        .map(|payload| FakeDrandSigner::decode(payload))
        .collect();
    assert_eq!(
        beacon
            .verify_entries(&entries, &BeaconEntry::default())
            .unwrap(),
        valid
    );
}
