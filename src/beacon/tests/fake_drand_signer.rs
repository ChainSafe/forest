// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use quick_protobuf::{BytesReader, MessageRead as _, MessageWrite as _, Writer};

use super::drand::{FIL_BLOCK_DELAY, FIL_GENESIS_TIME};
use super::fake_drand_server::FakeDrandChain;
use crate::beacon::{
    Beacon, BeaconEntry, ChainInfo, DrandBeacon, DrandConfig, DrandNetwork,
    drand_pb::PublicRandResponse, signatures::sign_unchained,
};
use crate::utils::encoding::hex;
use blstrs::{G2Projective, Scalar};
use group::{Curve, Group};

/// Signs any round of a `quicknet`-like chain with a made-up key, and encodes it as `drand`
/// gossips it.
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
        let quicknet = FakeDrandChain::quicknet().info;
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
                network_type: DrandNetwork::Quicknet, // unchained
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
        self.encode_raw(round, self.entry(round).signature())
    }

    /// A gossiped `PublicRandResponse` with any signature.
    pub fn encode_raw(&self, round: u64, signature: &[u8]) -> Vec<u8> {
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
        DrandBeacon::new(FIL_GENESIS_TIME, FIL_BLOCK_DELAY, &self.config)
    }
}

// Entries decoded from the gossip encoding verify against the fake chain's key.
#[test]
fn fake_drand_entries_verify() {
    let d = FakeDrandSigner::new();
    let entries: Vec<_> = (1..=5)
        .map(|round| FakeDrandSigner::decode(&d.encode(round)))
        .collect();
    assert!(
        d.beacon()
            .verify_entries(&entries, &BeaconEntry::default())
            .unwrap()
    );
}
