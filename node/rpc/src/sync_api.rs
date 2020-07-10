// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::RpcState;
use blocks::gossip_block::json::GossipBlockJson;
use blockstore::BlockStore;
use chain_sync::SyncState;
use cid::json::CidJson;
use encoding::Cbor;
use forest_libp2p::{NetworkMessage, Topic, PUBSUB_BLOCK_STR};
use jsonrpc_v2::{Data, Error as JsonRpcError, Params};
use serde::Serialize;

#[derive(Serialize)]
pub struct RPCSyncState {
    #[serde(rename = "ActiveSyncs")]
    active_syncs: Vec<SyncState>,
}

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
    data: Data<RpcState<DB>>,
) -> Result<RPCSyncState, JsonRpcError> {
    let state = data.sync_state.read().await.clone();
    Ok(RPCSyncState {
        active_syncs: vec![state],
    })
}

/// Submits block to be sent through gossipsub.
pub(crate) async fn sync_submit_block<DB: BlockStore + Send + Sync + 'static>(
    data: Data<RpcState<DB>>,
    Params((GossipBlockJson(blk),)): Params<(GossipBlockJson,)>,
) -> Result<(), JsonRpcError> {
    // TODO validate by constructing full block and validate (cids of messages could be invalid)
    // Also, we may want to indicate to chain sync process specifically about this block
    data.network_send
        .send(NetworkMessage::PubsubMessage {
            topic: Topic::new(format!("{}/{}", PUBSUB_BLOCK_STR, data.network_name)),
            message: blk.marshal_cbor().map_err(|e| e.to_string())?,
        })
        .await;
    Ok(())
}
