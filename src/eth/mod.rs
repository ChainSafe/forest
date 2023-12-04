// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub type Address = ethereum_types::Address;

pub type Hash = ethereum_types::H160;

#[derive(Default, Clone)]
pub struct BlockNumberOrHash {
    pub predefined_block: String,
    pub block_number: u64,
    pub block_hash: Hash,
    pub require_canonical: bool,
}
