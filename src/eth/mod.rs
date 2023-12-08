// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use cid::{
    multihash::{self, MultihashDigest},
    Cid,
};
use std::{fmt, str::FromStr};

#[derive(Default, Clone)]
pub struct Hash(pub ethereum_types::H256);

impl Hash {
    // Should ONLY be used for blocks and Filecoin messages. Eth transactions expect a different hashing scheme.
    pub fn to_cid(&self) -> cid::Cid {
        let mh = multihash::Code::Blake2b256.digest(self.0.as_bytes());
        Cid::new_v1(fvm_ipld_encoding::DAG_CBOR, mh)
    }
}

impl FromStr for Hash {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Hash(ethereum_types::H256::from_str(s)?))
    }
}

#[derive(Default, Clone)]
pub enum Predefined {
    Earliest,
    Pending,
    #[default]
    Latest,
}

impl fmt::Display for Predefined {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let s = match self {
            Predefined::Earliest => "earliest",
            Predefined::Pending => "pending",
            Predefined::Latest => "latest",
        };
        write!(f, "{}", s)
    }
}

#[allow(dead_code)]
#[derive(Clone)]
pub enum BlockNumberOrHash {
    PredefinedBlock(Predefined),
    BlockNumber(u64),
    BlockHash(Hash, bool),
}

impl BlockNumberOrHash {
    pub fn from_predefined(predefined: Predefined) -> Self {
        Self::PredefinedBlock(predefined)
    }

    pub fn from_block_number(number: u64) -> Self {
        Self::BlockNumber(number)
    }
}
