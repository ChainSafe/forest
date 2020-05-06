// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub use super::drand_api::common::{
    GroupPacket as ProtoGroup, GroupRequest, Identity as ProtoIdentity,
};
use crate::identity::Identity;

use bls_signatures::{PublicKey, Serialize};
use std::convert::TryFrom;

/// The result of calling the Group API call to a Drand node.
/// This is the the working Group of the Drand network.
#[derive(Debug, Clone)]
pub struct Group {
    /// Number of signatures
    pub threshold: u32,
    /// Time between Drand entries
    pub period: u32,
    /// Nodes on the Drand Network
    pub nodes: Vec<Identity>,
    /// Public keys of signers
    pub public_key: Vec<PublicKey>,
    pub transition_time: u64,
    /// Genesis time of Drand network
    pub genesis_time: u64,
    /// Genesis seed of the Drand network
    pub genesis_seed: Vec<u8>,
}

impl TryFrom<ProtoGroup> for Group {
    type Error = String;
    fn try_from(proto_group: ProtoGroup) -> Result<Self, Self::Error> {
        let identities: Vec<Identity> = proto_group
            .nodes
            .into_iter()
            .map(Identity::try_from)
            .collect::<Result<_, _>>()
            .map_err(|e| e.to_string())?;
        let n = identities.len();
        let threshold = proto_group.threshold;
        if threshold < minimum_threshold(n) {
            return Err("invaliud threshold".to_owned());
        }
        let genesis_time = proto_group.genesis_time;
        if genesis_time == 0 {
            return Err("genesis time is zero".to_owned());
        }
        let period = proto_group.period;
        if period == 0 {
            return Err("period time is zero".to_owned());
        }
        let dist: Vec<PublicKey> = proto_group
            .dist_key
            .into_iter()
            .map(|k| PublicKey::from_bytes(&k))
            .collect::<std::result::Result<_, _>>()
            .unwrap();

        Ok(Self {
            threshold,
            genesis_time,
            period,
            nodes: identities,
            transition_time: proto_group.transition_time,
            public_key: dist,
            genesis_seed: proto_group.genesis_seed,
        })
    }
}

#[inline]
fn minimum_threshold(n: usize) -> u32 {
    //	return int(math.Floor(float64(n)/2.0) + 1)
    ((n as f64 / 2.0).floor() + 1.0) as u32
}
