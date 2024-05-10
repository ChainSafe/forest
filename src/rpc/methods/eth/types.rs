// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;

pub const ETH_ADDRESS_LENGTH: usize = 20;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EthAddress(pub [u8; ETH_ADDRESS_LENGTH]);
lotus_json_with_self!(EthAddress);
