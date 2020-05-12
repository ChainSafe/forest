// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::drand_api::common::Identity as ProtoIdentity;
use bls_signatures::{PublicKey, Serialize};
use std::convert::TryFrom;

/// Information about the participants in the Drand network
#[derive(Debug, Clone)]
pub struct Identity {
    pub address: String,
    pub key: PublicKey,
    pub tls: bool,
}

impl TryFrom<ProtoIdentity> for Identity {
    type Error = Box<dyn std::error::Error>;

    fn try_from(proto_identity: ProtoIdentity) -> Result<Self, Self::Error> {
        Ok(Self {
            address: proto_identity.address,
            key: PublicKey::from_bytes(&proto_identity.key)?,
            tls: proto_identity.tls,
        })
    }
}
