// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{ApiInfo, RpcRequest};

/// Client calls should use [`crate::rpc::RpcMethod`]'s way of constructing [`RpcRequest`].
/// `Filecoin.ChainNotify` is an exception because it is a subscription method, so falls outside
/// of that abstraction.
/// See <https://github.com/ChainSafe/forest/issues/4032> for more information.
impl ApiInfo {
    pub fn chain_notify_req() -> RpcRequest<()> {
        RpcRequest::new(crate::rpc::chain::CHAIN_NOTIFY, ())
    }
}
