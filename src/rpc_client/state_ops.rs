// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{ApiInfo, RpcRequest};
use crate::rpc::types::*;
use crate::{
    rpc::state::*,
    shim::{address::Address, message::MethodNum},
};
use libipld_core::ipld::Ipld;

impl ApiInfo {
    pub fn state_decode_params_req(
        recipient: Address,
        method_number: MethodNum,
        params: Vec<u8>,
        tsk: ApiTipsetKey,
    ) -> RpcRequest<Ipld> {
        RpcRequest::new(STATE_DECODE_PARAMS, (recipient, method_number, params, tsk))
    }
}
