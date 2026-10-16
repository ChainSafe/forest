// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::beacon::{
    Beacon, BeaconEntry, ChainInfo, DrandBeacon, DrandConfig, DrandNetwork, PublicRandResponse,
};
use crate::utils::encoding::hex;
use blstrs::{G1Projective, G2Projective, Scalar};
use group::{Curve, Group};

const FAKE_DRAND_GENESIS_TIME: i32 = 1_692_803_367;
const FAKE_DRAND_PERIOD: i32 = 3;

const TEST_FIL_GENESIS_TIME: u64 = 1_598_306_400;
const TEST_FIL_BLOCK_DELAY: u64 = 30;

pub struct FakeDrandSigner {
    secret: Scalar,
    config: DrandConfig<'static>,
}

impl FakeDrandSigner {
    pub fn new() -> Self {
        let secret = Scalar::from(0xC0FFEEu64);
        let public = G2Projective::generator() * secret;
        let public_key = hex::encode(public.to_affine().to_compressed());
        Self {
            secret,
            config: DrandConfig {
                servers: vec![],
                chain_info: ChainInfo {
                    public_key: public_key.into(),
                    period: FAKE_DRAND_PERIOD,
                    genesis_time: FAKE_DRAND_GENESIS_TIME,
                    hash: "0011".repeat(16).into(),
                    group_hash: "00".repeat(32).into(),
                },
                network_type: DrandNetwork::Quicknet, // unchained
                // Would clash with the real quicknet config's collector.
                register_metrics: false,
            },
        }
    }

    // sign H(round) on G1, exactly what `verify_entries` checks for unchained.
    pub fn entry(&self, round: u64) -> BeaconEntry {
        let msg = BeaconEntry::message_unchained(round);
        let point =
            G1Projective::hash_to_curve(msg.as_ref(), crate::beacon::signatures::CSUITE_G1, &[]);
        let point = point * self.secret;
        BeaconEntry::new(round, point.to_affine().to_compressed().to_vec())
    }

    // encode PublicRandResponse to protobuf
    pub fn to_protobuf(&self, round: u64) -> Vec<u8> {
        self.to_protobuf_raw(round, self.entry(round).signature())
    }

    /// A `PublicRandResponse` payload with any signature.
    pub fn to_protobuf_raw(&self, round: u64, signature: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut w = quick_protobuf::Writer::new(&mut out);
        quick_protobuf::MessageWrite::write_message(
            &PublicRandResponse {
                round,
                signature: signature.to_vec(),
            },
            &mut w,
        )
        .unwrap();
        out
    }

    pub fn beacon(&self) -> DrandBeacon {
        DrandBeacon::new(TEST_FIL_GENESIS_TIME, TEST_FIL_BLOCK_DELAY, &self.config)
    }
}

// Entries decoded from the gossip encoding verify against the fake chain's key.
#[test]
fn fake_drand_entries_verify() {
    let d = FakeDrandSigner::new();
    let entries: Vec<_> = (1..=5)
        .map(|r| {
            let payload = d.to_protobuf(r);
            let mut reader = quick_protobuf::BytesReader::from_bytes(&payload);
            let response = <PublicRandResponse as quick_protobuf::MessageRead>::from_reader(
                &mut reader,
                &payload,
            )
            .unwrap();
            BeaconEntry::new(response.round, response.signature)
        })
        .collect();
    assert!(
        d.beacon()
            .verify_entries(&entries, &BeaconEntry::default())
            .unwrap()
    );
}
