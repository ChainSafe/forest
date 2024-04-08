// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::{
    rpc::{
        gas_api::*,
        types::{ApiTipsetKey, MessageSendSpec},
    },
    shim::message::Message,
};

use super::{ApiInfo, JsonRpcError, RpcRequest};

impl ApiInfo {
    pub fn gas_estimate_message_gas_req(
        msg: Message,
        spec: Option<MessageSendSpec>,
        tsk: ApiTipsetKey,
    ) -> RpcRequest<Message> {
        RpcRequest::new(GAS_ESTIMATE_MESSAGE_GAS, (msg, spec, tsk))
    }

    pub async fn gas_estimate_message_gas(
        &self,
        msg: Message,
        spec: Option<MessageSendSpec>,
        tsk: ApiTipsetKey,
    ) -> Result<Message, JsonRpcError> {
        self.call(Self::gas_estimate_message_gas_req(msg, spec, tsk))
            .await
    }
}
