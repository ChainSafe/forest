// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::api::PublicRandRequest;
use super::api_grpc::PublicClient;
use super::beacon_entries::BeaconEntry;
use super::common::{ GroupRequest};
use super::group::Group;

use bls_signatures::{PublicKey, Serialize, Signature};
use byteorder::{BigEndian, WriteBytesExt};
use grpc::ClientStub;
use grpc::RequestOptions;
use std::convert::TryFrom;
use std::error;
use std::sync::Arc;
use tls_api_openssl::TlsConnector;

use sha2::Digest;
// struct DrandPeer {
//     addr: String,
//     tls: bool,
// }
#[derive(Clone, Debug)]
pub struct DistPublic {
    pub coefficients: [Vec<u8>; 3]
}
impl DistPublic {
    pub fn key(&self) -> PublicKey {
        PublicKey::from_bytes(&self.coefficients[0]).unwrap()
    }
}
pub struct DrandBeacon {
    client: PublicClient,
    // peers: Vec<DrandPeer>,
    pub_key: DistPublic,
    interval: u64,

    drand_gen_time: u64,

    fil_gen_time: u64,
    fil_round_time: u64,
}

impl DrandBeacon {
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

        // TODO: append all peers

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
            // TODO: Is this right?
            round - 1,
        ))
    }

    pub fn verify_entry(&self, curr: BeaconEntry, prev: BeaconEntry) -> bool {
        // TODO: Handle Genesis

        //Hash the messages
        let mut msg: Vec<u8> = Vec::with_capacity(112);
        msg.write_u64::<BigEndian>(prev.round()).unwrap();
        msg.extend_from_slice(prev.data());
        msg.write_u64::<BigEndian>(curr.round()).unwrap();
        // the message
        let digest = sha2::Sha256::digest(&msg);
        let digest = bls_signatures::hash(&digest);

        //verify messages

        //signature
        let sig = Signature::from_bytes(curr.data()).unwrap();
        bls_signatures::verify(&sig, &[digest], &[self.pub_key.key()])
        // TODO: Cache this
    }

}

#[cfg(test)]
mod test {
    use crate::api_grpc::PublicClient;
    use crate::common::GroupRequest;
    use crate::group::Group;
    use grpc::ClientStub;
    use std::convert::TryFrom;
    use std::sync::Arc;
    use super::*;
    use bls_signatures::PublicKey;

    #[async_std::test]
    async fn t1() {
        let client = grpc::ClientBuilder::new("drand-test1.nikkolasg.xyz", 5001)
            .tls::<tls_api_openssl::TlsConnector>()
            .build()
            .unwrap();
        let client = PublicClient::with_client(Arc::new(client));

        let req = GroupRequest::new();
        let resp = client
            .group(grpc::RequestOptions::new(), req)
            .drop_metadata()
            .await
            .unwrap();

        let group: Group = Group::try_from(resp).unwrap();
        println!("{:?}", group);
    }

    #[async_std::test]
    async fn f2() {
        let x = [ 
            hex::decode("a2a34cf9a6be2f66b5385caa520364f994ae7dbac08371ffaca575dfb3e04c8e149b32dc78f077322c613a151dc07440").unwrap(),
            hex::decode("b0c5baca062191f13099229c9a229a9946204f74fc28baa212745243553ab1f50b581b2086e24374ceb40fe34bd23ca2").unwrap(),
            hex::decode("a9c6449cf647e0a0ffaf1e01277e2821213c80310165990daf77610208abfa0ce56c7e40995e26aff3873c624362ca78").unwrap(),
        ];
        let dist_pub = DistPublic {
            coefficients: x
        };
        let beacon = DrandBeacon::new(dist_pub, 1, 25).await.unwrap();


        // let e1 = beacon.entry(1).await.unwrap();
        let e2 = beacon.entry(2).await.unwrap();
        let e3 = beacon.entry(3).await.unwrap();

        println!("Verify e1, e2: {}", beacon.verify_entry(e3, e2))
    }

    #[async_std::test]
    async fn f4() {
        
    }

    #[test]
    fn f3() {
        let x = [ 
            "a2a34cf9a6be2f66b5385caa520364f994ae7dbac08371ffaca575dfb3e04c8e149b32dc78f077322c613a151dc07440",
            "b0c5baca062191f13099229c9a229a9946204f74fc28baa212745243553ab1f50b581b2086e24374ceb40fe34bd23ca2",
            "a9c6449cf647e0a0ffaf1e01277e2821213c80310165990daf77610208abfa0ce56c7e40995e26aff3873c624362ca78",
        ];
 
        let mut v = Vec::new();
        v.extend_from_slice(&hex::decode(x[0]).unwrap());
        // v.extend_from_slice(&hex::decode(x[1]).unwrap());
        // v.extend_from_slice(&hex::decode(x[2]).unwrap());
        println!("Sigze: {}", v.len());

        let k = PublicKey::from_bytes(&v);
        println!("{:?}", k)
    }
}
