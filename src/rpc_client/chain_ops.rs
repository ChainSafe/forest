// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::time::Duration;

use crate::rpc::{chain_api::ChainGetPath, types::*, RpcMethod};
use crate::{
    blocks::{CachingBlockHeader, Tipset, TipsetKey},
    rpc::chain_api::*,
    rpc::types::BlockMessages,
    shim::clock::ChainEpoch,
};
use cid::Cid;

use super::{ApiInfo, JsonRpcError, RpcRequest};

impl ApiInfo {
    pub async fn chain_head(&self) -> Result<Tipset, JsonRpcError> {
        self.call(Self::chain_head_req()).await
    }

    pub fn chain_head_req() -> RpcRequest<Tipset> {
        RpcRequest::new(CHAIN_HEAD, ())
    }

    pub async fn chain_get_block(&self, cid: Cid) -> Result<CachingBlockHeader, JsonRpcError> {
        self.call(Self::chain_get_block_req(cid)).await
    }

    pub fn chain_get_block_req(cid: Cid) -> RpcRequest<CachingBlockHeader> {
        RpcRequest::new(CHAIN_GET_BLOCK, (cid,))
    }

    pub fn chain_get_block_messages_req(cid: Cid) -> RpcRequest<BlockMessages> {
        RpcRequest::new(CHAIN_GET_BLOCK_MESSAGES, (cid,))
    }

    /// Get tipset at epoch. Pick younger tipset if epoch points to a
    /// null-tipset. Only tipsets below the given `head` are searched. If `head`
    /// is null, the node will use the heaviest tipset.
    pub async fn chain_get_tipset_by_height(
        &self,
        epoch: ChainEpoch,
        head: ApiTipsetKey,
    ) -> Result<Tipset, JsonRpcError> {
        self.call(Self::chain_get_tipset_by_height_req(epoch, head))
            .await
    }

    pub fn chain_get_tipset_by_height_req(
        epoch: ChainEpoch,
        head: ApiTipsetKey,
    ) -> RpcRequest<Tipset> {
        RpcRequest::new(CHAIN_GET_TIPSET_BY_HEIGHT, (epoch, head))
    }

    pub fn chain_get_tipset_after_height_req(
        epoch: ChainEpoch,
        head: ApiTipsetKey,
    ) -> RpcRequest<Tipset> {
        RpcRequest::new_v1(CHAIN_GET_TIPSET_AFTER_HEIGHT, (epoch, head))
    }

    #[allow(unused)] // consistency
    pub async fn chain_get_tipset(&self, tsk: TipsetKey) -> Result<Tipset, JsonRpcError> {
        self.call(Self::chain_get_tipset_req(tsk)).await
    }

    pub fn chain_get_tipset_req(tsk: TipsetKey) -> RpcRequest<Tipset> {
        RpcRequest::new(CHAIN_GET_TIPSET, (tsk,))
    }

    pub async fn chain_get_genesis(&self) -> Result<Option<Tipset>, JsonRpcError> {
        self.call(Self::chain_get_genesis_req()).await
    }

    pub fn chain_get_genesis_req() -> RpcRequest<Option<Tipset>> {
        RpcRequest::new(CHAIN_GET_GENESIS, ())
    }

    pub async fn chain_set_head(&self, new_head: TipsetKey) -> Result<(), JsonRpcError> {
        self.call(Self::chain_set_head_req(new_head)).await
    }

    pub fn chain_set_head_req(new_head: TipsetKey) -> RpcRequest<()> {
        RpcRequest::new(CHAIN_SET_HEAD, (new_head,))
    }

    pub async fn chain_export(
        &self,
        params: ChainExportParams,
    ) -> Result<ChainExportResult, JsonRpcError> {
        self.call(Self::chain_export_req(params)).await
    }

    pub fn chain_export_req(params: ChainExportParams) -> RpcRequest<ChainExportResult> {
        // snapshot export could take a few hours on mainnet
        RpcRequest::new(CHAIN_EXPORT, params).with_timeout(Duration::MAX)
    }

    pub async fn chain_read_obj(&self, cid: Cid) -> Result<Vec<u8>, JsonRpcError> {
        self.call(Self::chain_read_obj_req(cid)).await
    }

    pub fn chain_read_obj_req(cid: Cid) -> RpcRequest<Vec<u8>> {
        RpcRequest::new(CHAIN_READ_OBJ, (cid,))
    }

    pub fn chain_get_path_req(from: TipsetKey, to: TipsetKey) -> RpcRequest<Vec<PathChange>> {
        RpcRequest::new(ChainGetPath::NAME, (from, to))
    }

    pub fn chain_has_obj_req(cid: Cid) -> RpcRequest<bool> {
        RpcRequest::new(CHAIN_HAS_OBJ, (cid,))
    }

    pub async fn chain_get_min_base_fee(
        &self,
        basefee_lookback: u32,
    ) -> Result<String, JsonRpcError> {
        self.call(Self::chain_get_min_base_fee_req(basefee_lookback))
            .await
    }

    pub fn chain_get_min_base_fee_req(basefee_lookback: u32) -> RpcRequest<String> {
        RpcRequest::new(CHAIN_GET_MIN_BASE_FEE, (basefee_lookback,))
    }

    pub fn chain_get_messages_in_tipset_req(tsk: TipsetKey) -> RpcRequest<Vec<ApiMessage>> {
        RpcRequest::new(CHAIN_GET_MESSAGES_IN_TIPSET, (tsk,))
    }

    pub fn chain_get_parent_messages_req(block_cid: Cid) -> RpcRequest<Vec<ApiMessage>> {
        RpcRequest::new(CHAIN_GET_PARENT_MESSAGES, (block_cid,))
    }

    pub fn chain_notify_req() -> RpcRequest<()> {
        RpcRequest::new(CHAIN_NOTIFY, ())
    }

    pub fn chain_get_parent_receipts_req(block_cid: Cid) -> RpcRequest<Vec<ApiReceipt>> {
        RpcRequest::new(CHAIN_GET_PARENT_RECEIPTS, (block_cid,))
    }
}
