// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc_api::eth_api::ETH_BLOCK_NUMBER;

use super::{ApiInfo, RpcRequest};

impl ApiInfo {
    pub fn eth_block_number_req() -> RpcRequest<String> {
        RpcRequest::new_v1(ETH_BLOCK_NUMBER, ())
    }
}
