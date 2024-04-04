// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::{
    message::SignedMessage,
    rpc::{mpool_api::*, RpcMethod},
};
use cid::Cid;

use super::{ApiInfo, JsonRpcError, RpcRequest};

impl ApiInfo {
    pub async fn mpool_pending(&self, cids: Vec<Cid>) -> Result<Vec<SignedMessage>, JsonRpcError> {
        self.call(Self::mpool_pending_req(cids)).await
    }

    pub fn mpool_pending_req(cids: Vec<Cid>) -> RpcRequest<Vec<SignedMessage>> {
        RpcRequest::new(MpoolPending::NAME, (cids,))
    }
}
