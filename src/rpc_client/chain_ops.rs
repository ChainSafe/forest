// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::{
    blocks::{BlockHeader, Tipset, TipsetKeys},
    lotus_json::LotusJson,
    rpc_api::chain_api::*,
    shim::clock::ChainEpoch,
};
use cid::Cid;
use jsonrpc_v2::Error;

use crate::rpc_client::call;

use super::{ApiInfo, RpcRequest};

impl ApiInfo {
    pub async fn chain_head(&self) -> Result<Tipset, Error> {
        let LotusJson(tipset) = self.call(CHAIN_HEAD, ()).await?;
        Ok(tipset)
    }

    pub async fn chain_get_block(&self, cid: Cid) -> Result<BlockHeader, Error> {
        let LotusJson(header) = self.call(CHAIN_GET_BLOCK, (LotusJson(cid),)).await?;
        Ok(header)
    }

    // Get tipset at epoch. Pick younger tipset if epoch points to a
    // null-tipset. Only tipsets below the given `head` are searched. If `head`
    // is null, the node will use the heaviest tipset.
    pub async fn chain_get_tipset_by_height(
        &self,
        epoch: ChainEpoch,
        head: TipsetKeys,
    ) -> Result<Tipset, Error> {
        let LotusJson(tipset) = self.call(CHAIN_GET_TIPSET_BY_HEIGHT, (epoch, head)).await?;
        Ok(tipset)
    }

    pub async fn chain_get_genesis(&self) -> Result<Option<Tipset>, Error> {
        let LotusJson(opt_gen) = self.call(CHAIN_GET_GENESIS, ()).await?;
        Ok(opt_gen)
    }
}

pub async fn chain_get_block(
    cid: ChainGetBlockParams,
    auth_token: &Option<String>,
) -> Result<ChainGetBlockResult, Error> {
    call(CHAIN_GET_BLOCK, cid, auth_token).await
}

pub fn chain_get_block_req(cid: Cid) -> RpcRequest<BlockHeader> {
    RpcRequest::new(CHAIN_GET_BLOCK, (cid,))
}

pub async fn chain_export(
    params: ChainExportParams,
    auth_token: &Option<String>,
) -> Result<ChainExportResult, Error> {
    call(CHAIN_EXPORT, params, auth_token).await
}

pub async fn chain_get_tipset_by_height(
    params: ChainGetTipsetByHeightParams,
    auth_token: &Option<String>,
) -> Result<ChainGetTipsetByHeightResult, Error> {
    call(CHAIN_GET_TIPSET_BY_HEIGHT, params, auth_token).await
}

pub fn chain_get_tipset_by_height_req(epoch: ChainEpoch, head: TipsetKeys) -> RpcRequest<Tipset> {
    RpcRequest::new(CHAIN_GET_TIPSET_BY_HEIGHT, (epoch, head))
}

pub async fn chain_get_genesis(
    auth_token: &Option<String>,
) -> Result<ChainGetGenesisResult, Error> {
    call(CHAIN_GET_GENESIS, (), auth_token).await
}

pub fn chain_get_genesis_req() -> RpcRequest<Option<Tipset>> {
    RpcRequest::new(CHAIN_GET_GENESIS, ())
}

pub async fn chain_head(auth_token: &Option<String>) -> Result<ChainHeadResult, Error> {
    call(CHAIN_HEAD, (), auth_token).await
}

pub fn chain_head_req() -> RpcRequest<Tipset> {
    RpcRequest::new(CHAIN_HEAD, ())
}

pub async fn chain_get_message(
    cid: ChainGetMessageParams,
    auth_token: &Option<String>,
) -> Result<ChainGetMessageResult, Error> {
    call(CHAIN_GET_MESSAGE, cid, auth_token).await
}

pub async fn chain_read_obj(
    cid: ChainReadObjParams,
    auth_token: &Option<String>,
) -> Result<ChainReadObjResult, Error> {
    call(CHAIN_READ_OBJ, cid, auth_token).await
}

pub async fn chain_set_head(
    params: ChainSetHeadParams,
    auth_token: &Option<String>,
) -> Result<ChainSetHeadResult, Error> {
    call(CHAIN_SET_HEAD, params, auth_token).await
}

pub async fn chain_get_min_base_fee(
    params: ChainGetMinBaseFeeParams,
    auth_token: &Option<String>,
) -> Result<ChainGetMinBaseFeeResult, Error> {
    call(CHAIN_GET_MIN_BASE_FEE, params, auth_token).await
}
