// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use forest_rpc_api::chain_api::*;
use jsonrpc_v2::Error;

use crate::call;

pub async fn chain_get_block(
    cid: ChainGetBlockParams,
    auth_token: &Option<String>,
) -> Result<ChainGetBlockResult, Error> {
    call(CHAIN_GET_BLOCK, cid, auth_token).await
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

pub async fn chain_get_genesis(
    auth_token: &Option<String>,
) -> Result<ChainGetGenesisResult, Error> {
    call(CHAIN_GET_GENESIS, (), auth_token).await
}

pub async fn chain_head(auth_token: &Option<String>) -> Result<ChainHeadResult, Error> {
    call(CHAIN_HEAD, (), auth_token).await
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

pub async fn chain_get_tipset(
    keys: ChainGetTipSetParams,
    auth_token: &Option<String>,
) -> Result<ChainGetTipSetResult, Error> {
    call(CHAIN_GET_TIPSET, keys, auth_token).await
}

pub async fn chain_get_tipset_hash(
    keys: ChainGetTipSetHashParams,
    auth_token: &Option<String>,
) -> Result<ChainGetTipSetHashResult, Error> {
    call(CHAIN_GET_TIPSET_HASH, keys, auth_token).await
}

pub async fn chain_validate_tipset_checkpoints(
    keys: ChainValidateTipSetCheckpointsParams,
    auth_token: &Option<String>,
) -> Result<ChainValidateTipSetCheckpointsResult, Error> {
    call(CHAIN_VALIDATE_TIPSET_CHECKPOINTS, keys, auth_token).await
}

pub async fn chain_get_name(
    params: ChainGetNameParams,
    auth_token: &Option<String>,
) -> Result<ChainGetNameResult, Error> {
    call(CHAIN_GET_NAME, params, auth_token).await
}
