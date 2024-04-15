// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{ApiInfo, RpcRequest};
use crate::rpc::{types::*, RpcMethod as _};
use num::BigInt;

impl ApiInfo {
    pub fn chain_notify_req() -> RpcRequest<()> {
        RpcRequest::new(crate::rpc::chain::CHAIN_NOTIFY, ())
    }

    pub fn chain_tipset_weight_req(tsk: ApiTipsetKey) -> RpcRequest<BigInt> {
        RpcRequest::new(crate::rpc::chain::ChainTipSetWeight::NAME, (tsk,))
    }
}
