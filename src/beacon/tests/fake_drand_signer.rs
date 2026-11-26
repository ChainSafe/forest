// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use quick_protobuf::{BytesReader, MessageRead as _, MessageWrite as _, Writer};

use super::drand::new_beacon_quicknet;
use crate::beacon::signatures::PublicKeyOnG2;
use crate::beacon::{
    Beacon, BeaconEntry, ChainInfo, DrandBeacon, DrandConfig, DrandNetwork,
    drand_pb::PublicRandResponse, signatures::sign_unchained,
};
use crate::networks::{QUICKNET_CHAIN_INFO, mainnet};
use crate::shim::clock::EPOCH_DURATION_SECONDS;
use crate::utils::encoding::hex;
use blstrs::{G2Projective, Scalar};
use group::Group;

/// Signs any round of a `quicknet`-like chain with a made-up key
/// using bls-unchained-g1-rfc9380 signature scheme then encodes it as [`BeaconEntry`]
pub struct FakeDrandSigner {
    secret: Scalar,
    config: DrandConfig<'static>,
}

impl FakeDrandSigner {
    pub fn new() -> Self {
        // Arbitrary: any non-zero scalar makes a valid key pair.
        let secret = Scalar::from(0xC0FFEEu64);
        let public = G2Projective::generator() * secret;
        let public_key = hex::encode(PublicKeyOnG2::from(public).as_bytes());
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
        BeaconEntry::new(round, signature.as_bytes().to_vec())
    }

    pub fn encode(&self, round: u64) -> Vec<u8> {
        Self::encode_raw(round, self.entry(round).signature())
    }

    fn decode(payload: &[u8]) -> BeaconEntry {
        let response =
            PublicRandResponse::from_reader(&mut BytesReader::from_bytes(payload), payload)
                .unwrap();
        BeaconEntry::new(response.round, response.signature)
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

    pub fn beacon(&self) -> DrandBeacon {
        DrandBeacon::new(
            mainnet::GENESIS_TIMESTAMP,
            EPOCH_DURATION_SECONDS as u64,
            &self.config,
        )
    }
}

// Two consecutive `quicknet` rounds captured from drand's `api.drand.sh` relay over gossipsub.
// The bytes also carry `randomness`, the field we skip.
const RELAY_PAYLOADS: [&str; 2] = [
    // https://api.drand.sh/52db9ba70e0cc0f6eaf7803dd07447a1f5477735fd3f661792ba94600c84e971/public/33233273
    "08f9b2ec0f123086665198ae4cc688d5612997d9a2d544ca83cf7ea72861038fad1992497fe95fd8d13c4ffd9474d55d79c46c8a95e7372220713a75c9c5b1c3aef49ba4d9f4e7261b99d3871ed3b430dfc882bcffe2835687",
    // https://api.drand.sh/52db9ba70e0cc0f6eaf7803dd07447a1f5477735fd3f661792ba94600c84e971/public/33233274
    "08fab2ec0f1230a5081cfe5ef2c4d72288af2d931e1189adc1cf6eedddbd838a79aa742b44b0eecea6fb608c1cdd35b87551d4a843e5d72220525fb5ec54032812a9091770c20f101706c93e3c8334edec80a2eadf43930f75",
];

#[test]
fn signed_rounds_verify() {
    let d = FakeDrandSigner::new();
    let entries: Vec<_> = (1..=5).map(|round| d.entry(round)).collect();
    assert!(
        d.beacon()
            .verify_entries(&entries, &BeaconEntry::default())
            .unwrap()
    );
}

#[test]
fn round_signed_for_another_round_does_not_verify() {
    let d = FakeDrandSigner::new();
    let payload = FakeDrandSigner::encode_raw(2, d.entry(3).signature());
    let entries = [FakeDrandSigner::decode(&payload)];
    assert!(
        !d.beacon()
            .verify_entries(&entries, &BeaconEntry::default())
            .unwrap()
    );
}

#[test]
fn relay_rounds_verify() {
    let entries: Vec<_> = RELAY_PAYLOADS
        .iter()
        .map(|payload| FakeDrandSigner::decode(&hex::decode(payload).unwrap()))
        .collect();
    assert!(
        new_beacon_quicknet()
            .verify_entries(&entries, &BeaconEntry::default())
            .unwrap()
    );
}
