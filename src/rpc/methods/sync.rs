// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod types;

use crate::blocks::{Block, FullTipset, GossipBlock};
use crate::chain;
use crate::chain_sync::{BlockValidationOutcome, SyncStatusReport, TipsetValidator};
use crate::libp2p::{IdentTopic, NetworkMessage, PUBSUB_BLOCK_STR};
use crate::prelude::*;
use crate::rpc::{ApiPaths, Ctx, Permission, RpcMethod, ServerError};
use enumflags2::BitFlags;
use fvm_ipld_encoding::to_vec;
use std::time::Duration;
use tokio::sync::broadcast::error::RecvError;
pub use types::*;

pub enum SyncCheckBad {}
impl RpcMethod<1> for SyncCheckBad {
    const NAME: &'static str = "Filecoin.SyncCheckBad";
    const PARAM_NAMES: [&'static str; 1] = ["cid"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: &'static str =
        "Returns the reason the given block is marked bad, or an empty string if it is not.";

    type Params = (Cid,);
    type Ok = String;

    async fn handle(
        ctx: Ctx,
        (cid,): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        Ok(ctx
            .bad_blocks
            .as_ref()
            .context("bad block cache is disabled")?
            .get(&cid)
            .map(|_| "bad".to_string())
            .unwrap_or_default())
    }
}

pub enum SyncMarkBad {}
impl RpcMethod<1> for SyncMarkBad {
    const NAME: &'static str = "Filecoin.SyncMarkBad";
    const PARAM_NAMES: [&'static str; 1] = ["cid"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Admin;
    const DESCRIPTION: &'static str = "Marks the block with the given CID as bad.";

    type Params = (Cid,);
    type Ok = ();

    async fn handle(
        ctx: Ctx,
        (cid,): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        ctx.bad_blocks
            .as_ref()
            .context("bad block cache is disabled")?
            .push(cid);
        Ok(())
    }
}

pub enum SyncSnapshotProgress {}
impl RpcMethod<0> for SyncSnapshotProgress {
    const NAME: &'static str = "Forest.SyncSnapshotProgress";
    const PARAM_NAMES: [&'static str; 0] = [];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: &'static str =
        "Returns the snapshot download progress. Return Null if the tracking isn't started";

    type Params = ();
    type Ok = SnapshotProgressState;

    async fn handle(
        ctx: Ctx,
        (): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        Ok(ctx.get_snapshot_progress_tracker())
    }
}

pub enum SyncStatus {}
impl RpcMethod<0> for SyncStatus {
    const NAME: &'static str = "Forest.SyncStatus";
    const PARAM_NAMES: [&'static str; 0] = [];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: &'static str = "Returns the current sync status of the node.";

    type Params = ();
    type Ok = Arc<SyncStatusReport>;

    async fn handle(
        ctx: Ctx,
        (): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let sync_status = ctx.sync_status.load().shallow_clone();
        Ok(sync_status)
    }
}

pub enum SyncSubmitBlock {}
impl RpcMethod<1> for SyncSubmitBlock {
    const NAME: &'static str = "Filecoin.SyncSubmitBlock";
    const PARAM_NAMES: [&'static str; 1] = ["block"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Write;
    const DESCRIPTION: &'static str = "Submits a newly created block to the network.";

    type Params = (GossipBlock,);
    type Ok = ();

    // NOTE: This currently skips all the sanity-checks and directly passes the message onto the
    // swarm.
    async fn handle(
        ctx: Ctx,
        (block_msg,): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let genesis_network_name = ctx.chain_config().network.genesis_name();
        let encoded_message = to_vec(&block_msg)?;
        let pubsub_block_str = format!("{PUBSUB_BLOCK_STR}/{genesis_network_name}");
        let (bls_messages, secp_messages) =
            chain::store::block_messages(ctx.db(), &block_msg.header)?;
        let block_cid = *block_msg.header.cid();
        let block = Block {
            header: block_msg.header,
            bls_messages,
            secp_messages,
        };
        let ts = FullTipset::from(block);
        let genesis_ts = ctx.chain_store().genesis_tipset();

        TipsetValidator(&ts)
            .validate(
                ctx.chain_store(),
                ctx.bad_blocks.as_ref(),
                &genesis_ts,
                ctx.chain_config().block_delay_secs,
            )
            .context("failed to validate the tipset")?;

        // Subscribe before injecting the tipset so the follower's verdict cannot be missed.
        let mut outcomes = ctx.block_validation_subscriber.subscribe();
        ctx.tipset_send
            .try_send(ts)
            .context("tipset queue is full")?;

        // Forest applies the tipset via the async follower, unlike Lotus whose `Syncer.Sync` is
        // synchronous. Wait (bounded by ~one block time) for the follower's verdict; for a block
        // that extends the head (the mining case) an applied verdict means the head has advanced,
        // so lotus-miner does not re-select the same base.
        let block_delay_secs = ctx.chain_config().block_delay_secs.into();
        let verdict = tokio::time::timeout(Duration::from_secs(block_delay_secs), async {
            loop {
                match outcomes.recv().await {
                    Ok((cid, outcome)) if cid == block_cid => return Some(outcome),
                    Ok(_) | Err(RecvError::Lagged(_)) => {}
                    Err(RecvError::Closed) => return None,
                }
            }
        })
        .await;

        // Publish the block unless the follower definitively rejected it: Lotus publishes after a
        // successful `Syncer.Sync` and errors on failure, and on no verdict within a block time we
        // still publish best-effort (the block passed the synchronous `TipsetValidator` and is not
        // known-bad; gossiping an unincluded block is not slashable, peers just reject it).
        match verdict {
            Ok(Some(BlockValidationOutcome::Rejected)) => {
                return Err(anyhow::anyhow!(
                    "submitted block {block_cid} was rejected during validation"
                )
                .into());
            }
            Ok(Some(BlockValidationOutcome::Applied)) | Ok(None) => {}
            Err(_elapsed) => tracing::warn!(
                %block_cid,
                block_delay_secs,
                "SyncSubmitBlock: no validation verdict within one block time; publishing best-effort"
            ),
        }
        ctx.network_send().send(NetworkMessage::PubsubMessage {
            topic: IdentTopic::new(pubsub_block_str),
            message: encoded_message,
        })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::chain_sync::NodeSyncStatus;
    use crate::libp2p::NetworkMessage;
    use crate::rpc::RPCState;
    use crate::rpc::test_utils::chain_store;

    fn ctx() -> (Arc<RPCState>, flume::Receiver<NetworkMessage>) {
        RPCState::for_tests(chain_store()).unwrap()
    }

    #[tokio::test]
    async fn set_check_bad() {
        let (ctx, _) = ctx();

        let cid = "bafy2bzacea3wsdh6y3a36tb3skempjoxqpuyompjbmfeyf34fi3uy6uue42v4"
            .parse::<Cid>()
            .unwrap();

        let reason = SyncCheckBad::handle(ctx.clone(), (cid,), &Default::default())
            .await
            .unwrap();
        assert_eq!(reason, "");

        // Mark that block as bad manually and check again to verify
        SyncMarkBad::handle(ctx.clone(), (cid,), &Default::default())
            .await
            .unwrap();

        let reason = SyncCheckBad::handle(ctx.clone(), (cid,), &Default::default())
            .await
            .unwrap();
        assert_eq!(reason, "bad");
    }

    #[tokio::test]
    async fn sync_status_test() {
        let (ctx, _) = ctx();

        let st_copy = ctx.sync_status.clone();

        let sync_status = SyncStatus::handle(ctx.clone(), (), &Default::default())
            .await
            .unwrap();
        assert_eq!(sync_status, st_copy.load().clone());

        // update cloned state
        st_copy.store(
            st_copy
                .load()
                .as_ref()
                .clone()
                .with_status(NodeSyncStatus::Syncing)
                .with_current_head_epoch(4)
                .into(),
        );

        let sync_status = SyncStatus::handle(ctx.clone(), (), &Default::default())
            .await
            .unwrap();

        assert_eq!(sync_status, st_copy.load().clone());
    }
}
