// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::{
    message::SignedMessage, rpc::mpool_api::*, rpc::types::MessageSendSpec, shim::address::Address,
    shim::message::Message,
};
use cid::Cid;

use super::{ApiInfo, JsonRpcError, RpcRequest};

impl ApiInfo {
    pub fn mpool_get_nonce_req(addr: Address) -> RpcRequest<u64> {
        RpcRequest::new(MPOOL_GET_NONCE, (addr,))
    }

    pub async fn mpool_push_message(
        &self,
        message: Message,
        specs: Option<MessageSendSpec>,
    ) -> Result<SignedMessage, JsonRpcError> {
        self.call(Self::mpool_push_message_req(message, specs))
            .await
    }

    pub fn mpool_push_message_req(
        message: Message,
        specs: Option<MessageSendSpec>,
    ) -> RpcRequest<SignedMessage> {
        RpcRequest::new(MPOOL_PUSH_MESSAGE, (message, specs))
    }

    pub async fn mpool_pending(&self, cids: Vec<Cid>) -> Result<Vec<SignedMessage>, JsonRpcError> {
        self.call(Self::mpool_pending_req(cids)).await
    }

    pub fn mpool_pending_req(cids: Vec<Cid>) -> RpcRequest<Vec<SignedMessage>> {
        RpcRequest::new(MPOOL_PENDING, (cids,))
    }
}
