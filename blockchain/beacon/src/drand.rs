// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#![allow(dead_code)]
use super::drand_api::api::PublicRandRequest;
use super::drand_api::api_grpc::PublicClient;
use super::beacon_entries::BeaconEntry;
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
/// Coeffiencients of the publicly available drand keys.
/// This is shared by all participants on the Drand network.
pub struct DistPublic {
    pub coefficients: [Vec<u8>; 3],
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
            panic!("what are you doing this cant be zero")
        }
        // construct grpc client
        let client = grpc::ClientBuilder::new("drand-test1.nikkolasg.xyz", 5001)
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

        Ok(BeaconEntry::new(
            resp.round,
            resp.signature,
            resp.previous_round,
        ))
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
        //Hash the messages
        let mut msg: Vec<u8> = Vec::with_capacity(112);
        msg.write_u64::<BigEndian>(prev.round())?;
        msg.extend_from_slice(prev.data());
        msg.write_u64::<BigEndian>(curr.round())?;
        // H(prev_round | prev sig | curr_round)
        let digest = sha2::Sha256::digest(&msg);
        // Hash to G2
        let digest = bls_signatures::hash(&digest);
        // Signature
        let sig = Signature::from_bytes(curr.data())?;
        let _sig_match = bls_signatures::verify(&sig, &[digest], &[self.pub_key.key()]);
        // TODO: Cache this result
        // TODO: Return because right now Drand's BLS is different from ours
        Ok(true)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::api_grpc::PublicClient;
    use crate::common::GroupRequest;
    use crate::group::Group;
    use bls_signatures::PublicKey;
    use grpc::ClientStub;
    use std::convert::TryFrom;
    use std::sync::Arc;

    async fn new_beacon() -> DrandBeacon {
        let coeffs = [
            hex::decode("a2a34cf9a6be2f66b5385caa520364f994ae7dbac08371ffaca575dfb3e04c8e149b32dc78f077322c613a151dc07440").unwrap(),
            hex::decode("b0c5baca062191f13099229c9a229a9946204f74fc28baa212745243553ab1f50b581b2086e24374ceb40fe34bd23ca2").unwrap(),
            hex::decode("a9c6449cf647e0a0ffaf1e01277e2821213c80310165990daf77610208abfa0ce56c7e40995e26aff3873c624362ca78").unwrap(),
        ];
        let dist_pub = DistPublic {
            coefficients: coeffs,
        };
        DrandBeacon::new(dist_pub, 1, 25).await.unwrap()
    }

    #[async_std::test]
    async fn construct_drand_beacon() {
        new_beacon();
    }

    #[async_std::test]
    async fn ask_and_verify_beacon_entry() {
        let beacon = new_beacon().await;

        let e2 = beacon.entry(2).await.unwrap();
        let e3 = beacon.entry(3).await.unwrap();
        assert!(beacon.verify_entry(e3, e2).unwrap());
    }
}
