// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{Message, Response};

#[derive(Debug, Clone, PartialEq)]
pub enum RPCRequest {
    Blocksync(Message),
}

impl RPCRequest {
    pub fn expect_response(&self) -> bool {
        match self {
            RPCRequest::Blocksync(_) => true,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum RPCResponse {
    SuccessBlocksync(Response),
}
