// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{ApiInfo, RpcRequest};
use crate::rpc::{
    miner::{BlockMessage, BlockTemplate, MinerCreateBlock},
    RpcMethod,
};

impl ApiInfo {
    pub fn miner_create_block_req(block_template: BlockTemplate) -> RpcRequest<BlockMessage> {
        RpcRequest::new(MinerCreateBlock::NAME, (block_template,))
    }
}
