// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::shim::message::Message;
use crate::{
    blocks::{BlockHeader, Tipset, TipsetKeys},
    lotus_json::LotusJson,
    rpc_api::chain_api::*,
    shim::clock::ChainEpoch,
};
use cid::Cid;
use jsonrpc_v2::Error;

use crate::rpc_client::call;

use super::{ApiInfo, JsonRpcError, RpcRequest};

impl ApiInfo {
    pub async fn chain_head(&self) -> Result<Tipset, Error> {
        self.call_req(Self::chain_head_req()).await
    }

    pub fn chain_head_req() -> RpcRequest<Tipset> {
        RpcRequest::new(CHAIN_HEAD, ())
    }

    pub async fn chain_get_block(&self, cid: Cid) -> Result<BlockHeader, JsonRpcError> {
        self.call_req_e(Self::chain_get_block_req(cid)).await
    }

    pub fn chain_get_block_req(cid: Cid) -> RpcRequest<BlockHeader> {
        RpcRequest::new(CHAIN_GET_BLOCK, (cid,))
    }

    // Get tipset at epoch. Pick younger tipset if epoch points to a
    // null-tipset. Only tipsets below the given `head` are searched. If `head`
    // is null, the node will use the heaviest tipset.
    pub async fn chain_get_tipset_by_height(
        &self,
        epoch: ChainEpoch,
        head: TipsetKeys,
    ) -> Result<Tipset, Error> {
        self.call_req(Self::chain_get_tipset_by_height_req(epoch, head))
            .await
    }

    pub fn chain_get_tipset_by_height_req(
        epoch: ChainEpoch,
        head: TipsetKeys,
    ) -> RpcRequest<Tipset> {
        RpcRequest::new(CHAIN_GET_TIPSET_BY_HEIGHT, (epoch, head))
    }

    pub async fn chain_get_genesis(&self) -> Result<Option<Tipset>, JsonRpcError> {
        self.call_req_e(Self::chain_get_genesis_req()).await
    }

    pub fn chain_get_genesis_req() -> RpcRequest<Option<Tipset>> {
        RpcRequest::new(CHAIN_GET_GENESIS, ())
    }

    pub async fn chain_set_head(&self, new_head: TipsetKeys) -> Result<(), JsonRpcError> {
        self.call_req_e(Self::chain_set_head_req(new_head)).await
    }

    pub fn chain_set_head_req(new_head: TipsetKeys) -> RpcRequest<()> {
        RpcRequest::new(CHAIN_SET_HEAD, (new_head,))
    }
}

pub async fn chain_export(
    params: ChainExportParams,
    auth_token: &Option<String>,
) -> Result<ChainExportResult, Error> {
    call(CHAIN_EXPORT, params, auth_token).await
}

pub fn chain_export_req(params: ChainExportParams) -> RpcRequest<ChainExportResult> {
    RpcRequest::new(CHAIN_EXPORT, params)
}

pub async fn chain_get_message(
    cid: ChainGetMessageParams,
    auth_token: &Option<String>,
) -> Result<ChainGetMessageResult, Error> {
    call(CHAIN_GET_MESSAGE, cid, auth_token).await
}

pub fn chain_get_message_req(cid: Cid) -> RpcRequest<Message> {
    RpcRequest::new(CHAIN_GET_MESSAGE, cid)
}

pub async fn chain_read_obj(
    cid: ChainReadObjParams,
    auth_token: &Option<String>,
) -> Result<ChainReadObjResult, Error> {
    call(CHAIN_READ_OBJ, cid, auth_token).await
}

pub fn chain_read_obj_req(cid: Cid) -> RpcRequest<String> {
    RpcRequest::new(CHAIN_READ_OBJ, cid)
}

pub async fn chain_get_min_base_fee(
    params: ChainGetMinBaseFeeParams,
    auth_token: &Option<String>,
) -> Result<ChainGetMinBaseFeeResult, Error> {
    call(CHAIN_GET_MIN_BASE_FEE, params, auth_token).await
}

pub fn chain_get_min_base_fee_req(basefee_lookback: u32) -> RpcRequest<ChainGetMinBaseFeeResult> {
    RpcRequest::new(CHAIN_GET_MIN_BASE_FEE, (basefee_lookback,))
}
