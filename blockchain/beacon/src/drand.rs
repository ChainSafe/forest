// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#![allow(dead_code)]
use super::beacon_entries::BeaconEntry;
use super::drand_api::api::PublicRandRequest;
use super::drand_api::api_grpc::PublicClient;
use super::drand_api::common::GroupRequest;
use super::group::Group;

use bls_signatures::{PublicKey, Serialize, Signature};
use byteorder::{BigEndian, WriteBytesExt};
use grpc::ClientStub;
use grpc::RequestOptions;
use sha2::Digest;
use std::convert::TryFrom;
use std::error;
use std::sync::Arc;
use tls_api_openssl::TlsConnector;

#[derive(Clone, Debug)]
/// Coeffiencients of the publicly available Drand keys.
/// This is shared by all participants on the Drand network.
pub struct DistPublic {
    pub coefficients: [Vec<u8>; 4],
}

impl DistPublic {
    pub fn key(&self) -> PublicKey {
        PublicKey::from_bytes(&self.coefficients[0]).unwrap()
    }
}

pub struct DrandBeacon {
    client: PublicClient,
    pub_key: DistPublic,
    interval: u64,
    drand_gen_time: u64,
    fil_gen_time: u64,
    fil_round_time: u64,
}

/// This struct allows you to talk to a Drand node over GRPC.
/// Use this to source randomness and to verify Drand beacon entries.
impl DrandBeacon {
    /// Construct a new DrandBeacon.
    pub async fn new(
        pub_key: DistPublic,
        genesis_ts: u64,
        interval: u64,
    ) -> Result<Self, Box<dyn error::Error>> {
        if genesis_ts == 0 {
            panic!("Genesis timestamp cannot be 0")
        }
        // construct grpc client
        // TODO: Allow to randomize between different drand servers
        let client = grpc::ClientBuilder::new("nicolas.drand.fil-test.net", 443)
            .tls::<TlsConnector>()
            .build()
            .unwrap();
        let client = PublicClient::with_client(Arc::new(client));

        // get nodes in group
        let req = GroupRequest::new();
        let group_resp = client
            .group(RequestOptions::new(), req)
            .drop_metadata()
            .await?;
        let group: Group = Group::try_from(group_resp)?;
        // TODO: Compare pubkeys with one in config

        Ok(Self {
            pub_key,
            client,
            interval: group.period as u64,
            drand_gen_time: group.genesis_time,
            fil_round_time: interval,
            fil_gen_time: genesis_ts,
        })
    }

    /// Returns a BeaconEntry given a round. It fetches the BeaconEntry from a Drand node over GRPC
    /// In the future, we will cache values, and support streaming.
    pub async fn entry(&self, round: u64) -> Result<BeaconEntry, Box<dyn error::Error>> {
        // TODO: Cache values into a database
        let mut req = PublicRandRequest::new();
        req.round = round;
        let resp = self
            .client
            .public_rand(grpc::RequestOptions::new(), req)
            .drop_metadata()
            .await?;

        Ok(BeaconEntry::new(resp.round, resp.signature, resp.round - 1))
    }

    /// Verify a new beacon entry against the most recent one before it.
    pub fn verify_entry(
        &self,
        curr: BeaconEntry,
        prev: BeaconEntry,
    ) -> Result<bool, Box<dyn error::Error>> {
        // TODO: Handle Genesis better
        if prev.round() == 0 {
            return Ok(true);
        }

        // Hash the messages
        let mut msg: Vec<u8> = Vec::with_capacity(104);
        msg.extend_from_slice(prev.data());
        msg.write_u64::<BigEndian>(curr.round())?;
        // H(prev sig | curr_round)
        let digest = sha2::Sha256::digest(&msg);
        // Hash to G2
        let digest = bls_signatures::hash(&digest);
        // Signature
        let sig = Signature::from_bytes(curr.data())?;
        let sig_match = bls_signatures::verify(&sig, &[digest], &[self.pub_key.key()]);
        // TODO: Cache this result
        Ok(sig_match)
    }
}

#[cfg(test)]
mod test {
    use super::*;

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

    #[async_std::test]
    async fn construct_drand_beacon() {
        new_beacon().await;
    }

    #[async_std::test]
    async fn ask_and_verify_beacon_entry() {
        let beacon = new_beacon().await;

        let e2 = beacon.entry(2).await.unwrap();
        let e3 = beacon.entry(3).await.unwrap();
        assert!(beacon.verify_entry(e3, e2).unwrap());
    }

    #[async_std::test]
    async fn ask_and_verify_beacon_entry_fail() {
        let beacon = new_beacon().await;

        let e2 = beacon.entry(2).await.unwrap();
        let e3 = beacon.entry(3).await.unwrap();
        assert!(!beacon.verify_entry(e2, e3).unwrap());
    }
}
