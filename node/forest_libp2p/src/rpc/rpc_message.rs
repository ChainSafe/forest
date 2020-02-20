// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{BlockSyncRequest, BlockSyncResponse};

/// RPCRequest payloads for request/response calls
#[derive(Debug, Clone, PartialEq)]
pub enum RPCRequest {
    Blocksync(BlockSyncRequest),
}

impl RPCRequest {
    pub fn expect_response(&self) -> bool {
        match self {
            RPCRequest::Blocksync(_) => true,
        }
    }
}

/// RPCResponse payloads for request/response calls
#[derive(Debug, Clone, PartialEq)]
pub enum RPCResponse {
    Blocksync(BlockSyncResponse),
}
