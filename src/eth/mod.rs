// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::fmt;

use crate::rpc_api::eth_api::Hash;

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
