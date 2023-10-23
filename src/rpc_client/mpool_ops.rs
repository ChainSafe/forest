// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::{
    message::SignedMessage,
    rpc_api::{data_types::MessageSendSpec, mpool_api::*},
    shim::message::Message,
};
use cid::Cid;
use jsonrpc_v2::Error;

use crate::rpc_client::call;

use super::RpcRequest;

pub async fn mpool_push_message(
    params: MpoolPushMessageParams,
    auth_token: &Option<String>,
) -> Result<MpoolPushMessageResult, Error> {
    call(MPOOL_PUSH_MESSAGE, params, auth_token).await
}

pub fn mpool_push_message_req(
    message: Message,
    specs: Option<MessageSendSpec>,
) -> RpcRequest<SignedMessage> {
    RpcRequest::new(MPOOL_PUSH_MESSAGE, (message, specs))
}

pub async fn mpool_pending(
    params: MpoolPendingParams,
    auth_token: &Option<String>,
) -> Result<MpoolPendingResult, Error> {
    call(MPOOL_PENDING, params, auth_token).await
}

pub fn mpool_pending_req(cids: Vec<Cid>) -> RpcRequest<Vec<SignedMessage>> {
    RpcRequest::new(MPOOL_PENDING, (cids,))
}
