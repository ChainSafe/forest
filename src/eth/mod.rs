// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::fmt;

pub type Address = ethereum_types::Address;

pub type Hash = ethereum_types::H160;

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

#[derive(Default, Clone)]
pub struct BlockNumberOrHash {
    pub predefined_block: Option<Predefined>,
    pub block_number: Option<u64>,
    pub block_hash: Option<Hash>,
    pub require_canonical: bool,
}

impl BlockNumberOrHash {
    pub fn from_predefined(predefined: Predefined) -> Self {
        Self {
            predefined_block: Some(predefined),
            block_number: None,
            block_hash: None,
            require_canonical: false,
        }
    }
}
