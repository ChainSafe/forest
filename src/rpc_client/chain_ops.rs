// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{ApiInfo, RpcRequest};

impl ApiInfo {
    pub fn chain_notify_req() -> RpcRequest<()> {
        RpcRequest::new(crate::rpc::chain::CHAIN_NOTIFY, ())
    }
}
