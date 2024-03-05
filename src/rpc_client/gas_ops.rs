// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::{
    blocks::TipsetKey,
    rpc_api::{data_types::MessageSendSpec, gas_api::*},
    shim::message::Message,
};

use super::{ApiInfo, JsonRpcError, RpcRequest};

impl ApiInfo {
    pub fn gas_estimate_message_gas_req(
        msg: Message,
        spec: Option<MessageSendSpec>,
        tsk: TipsetKey,
    ) -> RpcRequest<Message> {
        RpcRequest::new(GAS_ESTIMATE_MESSAGE_GAS, (msg, spec, tsk))
    }

    pub async fn gas_estimate_message_gas(
        &self,
        msg: Message,
        spec: Option<MessageSendSpec>,
        tsk: TipsetKey,
    ) -> Result<Message, JsonRpcError> {
        self.call(Self::gas_estimate_message_gas_req(msg, spec, tsk))
            .await
    }
}
