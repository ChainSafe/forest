// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::path::PathBuf;

use crate::rpc::types::*;
use crate::{
    rpc::state::*,
    shim::{address::Address, message::MethodNum},
};
use cid::Cid;
use libipld_core::ipld::Ipld;

use super::{ApiInfo, RpcRequest, ServerError};

impl ApiInfo {
    pub async fn state_fetch_root(
        &self,
        root: Cid,
        opt_path: Option<PathBuf>,
    ) -> Result<String, ServerError> {
        self.call(Self::state_fetch_root_req(root, opt_path)).await
    }

    pub fn state_fetch_root_req(root: Cid, opt_path: Option<PathBuf>) -> RpcRequest<String> {
        RpcRequest::new(STATE_FETCH_ROOT, (root, opt_path))
    }

    pub fn state_decode_params_req(
        recipient: Address,
        method_number: MethodNum,
        params: Vec<u8>,
        tsk: ApiTipsetKey,
    ) -> RpcRequest<Ipld> {
        RpcRequest::new(STATE_DECODE_PARAMS, (recipient, method_number, params, tsk))
    }

    pub fn state_search_msg_limited_req(
        msg_cid: Cid,
        limit_epoch: i64,
    ) -> RpcRequest<Option<MessageLookup>> {
        RpcRequest::new(STATE_SEARCH_MSG_LIMITED, (msg_cid, limit_epoch))
    }
}
