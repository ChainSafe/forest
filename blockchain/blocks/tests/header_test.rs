// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//#![cfg(test)]
#![cfg(feature = "json")]

use address::Address;
use crypto::{Signature, Signer};
use forest_blocks::BlockHeader;

// use header::{
//     self,
//     json::{BlockHeaderJson, BlockHeaderJsonRef},
// };

use serde::{Deserialize, Serialize};
use serde_json::{from_str, to_string};
use std::error::Error;
use test_utils::construct_ticket;
use vm::Serialized;

#[test]

fn blockheader_symmetric_json() {}
