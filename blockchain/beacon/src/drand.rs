// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::api::PublicRandRequest;
use super::api_grpc::PublicClient;
use super::beacon_entries::BeaconEntry;
use super::common::{GroupPacket as ProtoGroup, GroupRequest, Identity as ProtoIdentity};
use super::group::Group;

use grpc::ClientStub;
use grpc::RequestOptions;

use std::convert::TryFrom;
use std::error;
use std::sync::Arc;
use tls_api_openssl::TlsConnector;

struct DrandPeer {
    addr: String,
    tls: bool,
}
pub struct DrandBeacon {
    client: PublicClient,
    // peers: Vec<DrandPeer>,

    // pubkey
    interval: u64,

    drand_gen_time: u64,

    fil_gen_time: u64,
    fil_round_time: u64,
}

impl DrandBeacon {
    pub async fn new(genesis_ts: u64, interval: u64) -> Result<Self, Box<dyn error::Error>> {
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
            //pubkey
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

    
}

#[cfg(test)]
mod test {
    pub use crate::api_grpc::PublicClient;
    pub use crate::common::GroupRequest;
    use crate::group::Group;
    use async_std::prelude::*;
    use grpc::ClientStub;
    use httpbis::ClientTlsOption;
    use std::convert::TryFrom;
    use std::sync::Arc;
    use tls_api::*;
    use tls_api_openssl::*;

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
}
