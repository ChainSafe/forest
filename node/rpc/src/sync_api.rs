// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::RpcState;
use blocks::header::json::BlockHeaderJson;
use blockstore::BlockStore;
use cid::json::CidJson;
use jsonrpc_v2::{Data, Error as JsonRpcError, Params};

/// Checks if a given block is marked as bad.
pub(crate) async fn sync_check_bad<DB: BlockStore + Send + Sync + 'static>(
    data: Data<RpcState<DB>>,
    Params(params): Params<(CidJson,)>,
) -> Result<String, JsonRpcError> {
    let (CidJson(cid),) = params;
    Ok(data.bad_blocks.peek(&cid).await.unwrap_or_default())
}

/// Marks a block as bad, meaning it will never be synced.
pub(crate) async fn sync_mark_bad<DB: BlockStore + Send + Sync + 'static>(
    data: Data<RpcState<DB>>,
    Params(params): Params<(CidJson,)>,
) -> Result<(), JsonRpcError> {
    let (CidJson(cid),) = params;
    data.bad_blocks
        .put(cid, "Marked bad manually through RPC API".to_string())
        .await;
    Ok(())
}

// TODO SyncIncomingBlocks (requires websockets)

/// Returns the current status of the ChainSync process.
pub(crate) async fn sync_state<DB: BlockStore + Send + Sync + 'static>(
    _data: Data<RpcState<DB>>,
) -> Result<(), JsonRpcError> {
    todo!()
}

/// Submits block to be sent through gossipsub.
pub(crate) async fn sync_submit_block<DB: BlockStore + Send + Sync + 'static>(
    _data: Data<RpcState<DB>>,
    Params(_params): Params<(BlockHeaderJson,)>,
) -> Result<(), JsonRpcError> {
    todo!()
}
