// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::shim::address::Address as FilecoinAddress;
use std::{fmt, str::FromStr};

#[derive(Default, Clone)]
pub struct Address(pub ethereum_types::Address);

#[derive(Default, Clone, PartialEq)]
pub struct BigInt(pub num::BigInt);

pub type Hash = ethereum_types::H160;

#[derive(Default, Clone)]
pub enum Predefined {
    Earliest,
    Pending,
    #[default]
    Latest,
}

impl Address {
    pub fn to_filecoin_address(&self) -> Result<FilecoinAddress, anyhow::Error> {
        if self.is_masked_id() {
            unimplemented!()
        } else {
            Ok(FilecoinAddress::new_delegated(
                FilecoinAddress::ETHEREUM_ACCOUNT_MANAGER_ACTOR.id()?,
                &self.0.as_bytes(),
            )?)
        }
    }

    pub fn is_masked_id(&self) -> bool {
        // TODO
        false
    }
}

impl FromStr for Address {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Address(
            ethereum_types::Address::from_str(s).map_err(|e| anyhow::anyhow!("{e}"))?,
        ))
    }
}

impl fmt::Display for BigInt {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "0x{:x}", self.0)
    }
}

impl FromStr for BigInt {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(BigInt(num::BigInt::from_str(s)?))
    }
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
