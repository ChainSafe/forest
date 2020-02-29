// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::blocksync::BlockSyncResponse;

/// RPCResponse payloads for request/response calls
#[derive(Debug, Clone, PartialEq)]
pub enum RPCResponse {
    Blocksync(BlockSyncResponse),
}
