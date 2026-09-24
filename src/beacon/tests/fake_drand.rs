use crate::beacon::{Beacon, BeaconEntry, ChainInfo, DrandBeacon, DrandConfig, DrandNetwork};
use blstrs::{G1Projective, G2Projective, Scalar};
use group::{Curve, Group};

pub const FAKE_DRAND_GENESIS_TIME: i32 = 1_692_803_367;
pub const FAKE_DRAND_PERIOD: i32 = 3;

pub const TEST_FIL_GENESIS_TIME: u64 = 1_598_306_400;
pub const TEST_FIL_BLOCK_DELAY: u64 = 30;

pub struct FakeDrand {
    secret: Scalar,
    config: DrandConfig<'static>,
}

impl FakeDrand {
    pub fn new(servers: Vec<url::Url>, period: i32, genesis_time: i32) -> Self {
        let secret = Scalar::from(0xC0FFEEu64);
        let public = G2Projective::generator() * secret;
        let public_key = hex::encode(public.to_affine().to_compressed());
        Self {
            secret,
            config: DrandConfig {
                servers,
                chain_info: ChainInfo {
                    public_key: public_key.into(),
                    period,
                    genesis_time,
                    hash: "0011".repeat(16).into(),
                    group_hash: "00".repeat(32).into(),
                },
                network_type: DrandNetwork::Quicknet, // unchained
                // The fixture builds beacons directly; registering a collector here
                // would clash with the one the real quicknet config registers.
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
        let entry = self.entry(round);
        let mut out = Vec::new();
        let mut w = quick_protobuf::Writer::new(&mut out);
        quick_protobuf::MessageWrite::write_message(
            &crate::beacon::drand_pb::PublicRandResponse {
                round,
                signature: entry.signature().to_vec(),
            },
            &mut w,
        )
        .unwrap();
        out
    }

    pub fn to_json(&self, round: u64) -> serde_json::Value {
        let entry = self.entry(round);
        serde_json::json!({
            "round": round,
            "randomness": "00".repeat(32),
            "signature": hex::encode(entry.signature()),
            "previous_signature": null,
        })
    }

    pub fn beacon(&self, genesis_ts: u64, block_delay: u64) -> DrandBeacon {
        DrandBeacon::new(genesis_ts, block_delay, &self.config)
    }

    pub fn chain_info_hash(&self) -> String {
        self.config.chain_info.hash.to_string()
    }
}

// just test the secret and public keys are correctly validating
#[test]
fn fake_drand_entries_verify() {
    let d = FakeDrand::new(vec![], FAKE_DRAND_PERIOD, FAKE_DRAND_GENESIS_TIME);
    let beacon = d.beacon(TEST_FIL_GENESIS_TIME, TEST_FIL_BLOCK_DELAY);
    let entries: Vec<_> = (1..=5).map(|r| d.entry(r)).collect();
    assert!(
        beacon
            .verify_entries(&entries, &BeaconEntry::default())
            .unwrap()
    );
}
