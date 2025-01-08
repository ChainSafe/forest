// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::blocks::{Block, FullTipset, GossipBlock};
use crate::libp2p::{IdentTopic, NetworkMessage, PUBSUB_BLOCK_STR};
use crate::lotus_json::{lotus_json_with_self, LotusJson};
use crate::rpc::{ApiPaths, Ctx, Permission, RpcMethod, ServerError};
use anyhow::{anyhow, Context as _};
use cid::Cid;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::to_vec;
use nunny::{vec as nonempty, Vec as NonEmpty};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::chain;
use crate::chain_sync::{SyncStage, TipsetValidator};

pub enum SyncCheckBad {}
impl RpcMethod<1> for SyncCheckBad {
    const NAME: &'static str = "Filecoin.SyncCheckBad";
    const PARAM_NAMES: [&'static str; 1] = ["cid"];
    const API_PATHS: ApiPaths = ApiPaths::V1;
    const PERMISSION: Permission = Permission::Read;

    type Params = (Cid,);
    type Ok = String;

    async fn handle(
        ctx: Ctx<impl Blockstore>,
        (cid,): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        Ok(ctx.bad_blocks.peek(&cid).unwrap_or_default())
    }
}

pub enum SyncMarkBad {}
impl RpcMethod<1> for SyncMarkBad {
    const NAME: &'static str = "Filecoin.SyncMarkBad";
    const PARAM_NAMES: [&'static str; 1] = ["cid"];
    const API_PATHS: ApiPaths = ApiPaths::V1;
    const PERMISSION: Permission = Permission::Admin;

    type Params = (Cid,);
    type Ok = ();

    async fn handle(
        ctx: Ctx<impl Blockstore>,
        (cid,): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        ctx.bad_blocks
            .put(cid, "Marked bad manually through RPC API".to_string());
        Ok(())
    }
}

pub enum SyncState {}
impl RpcMethod<0> for SyncState {
    const NAME: &'static str = "Filecoin.SyncState";
    const PARAM_NAMES: [&'static str; 0] = [];
    const API_PATHS: ApiPaths = ApiPaths::V1;
    const PERMISSION: Permission = Permission::Read;

    type Params = ();
    type Ok = RPCSyncState;

    async fn handle(ctx: Ctx<impl Blockstore>, (): Self::Params) -> Result<Self::Ok, ServerError> {
        let active_syncs = nonempty![ctx.sync_state.as_ref().read().clone()];
        Ok(RPCSyncState { active_syncs })
    }
}

pub enum SyncSubmitBlock {}
impl RpcMethod<1> for SyncSubmitBlock {
    const NAME: &'static str = "Filecoin.SyncSubmitBlock";
    const PARAM_NAMES: [&'static str; 1] = ["blk"];
    const API_PATHS: ApiPaths = ApiPaths::V1;
    const PERMISSION: Permission = Permission::Write;

    type Params = (GossipBlock,);
    type Ok = ();

    // NOTE: This currently skips all the sanity-checks and directly passes the message onto the
    // swarm.
    async fn handle(
        ctx: Ctx<impl Blockstore>,
        (block_msg,): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        if !matches!(ctx.sync_state.read().stage(), SyncStage::Complete) {
            Err(anyhow!("the node isn't in 'follow' mode"))?
        }
        let encoded_message = to_vec(&block_msg)?;
        let pubsub_block_str = format!("{}/{}", PUBSUB_BLOCK_STR, ctx.network_name);
        let (bls_messages, secp_messages) =
            chain::store::block_messages(&ctx.chain_store().db, &block_msg.header)?;
        let block = Block {
            header: block_msg.header.clone(),
            bls_messages,
            secp_messages,
        };
        let ts = FullTipset::from(block);
        let genesis_ts = ctx.chain_store().genesis_tipset();

        TipsetValidator(&ts)
            .validate(
                ctx.chain_store(),
                Some(&ctx.bad_blocks),
                &genesis_ts,
                ctx.chain_config().block_delay_secs,
            )
            .context("failed to validate the tipset")?;

        ctx.tipset_send
            .try_send(Arc::new(ts.into_tipset()))
            .context("tipset queue is full")?;

        ctx.network_send().send(NetworkMessage::PubsubMessage {
            topic: IdentTopic::new(pubsub_block_str),
            message: encoded_message,
        })?;
        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub struct RPCSyncState {
    #[schemars(with = "LotusJson<Vec<crate::chain_sync::SyncState>>")]
    #[serde(with = "crate::lotus_json")]
    pub active_syncs: NonEmpty<crate::chain_sync::SyncState>,
}
lotus_json_with_self!(RPCSyncState);

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::blocks::RawBlockHeader;
    use crate::blocks::{CachingBlockHeader, Tipset};
    use crate::chain::ChainStore;
    use crate::chain_sync::network_context::SyncNetworkContext;
    use crate::chain_sync::{SyncConfig, SyncStage};
    use crate::db::MemoryDB;
    use crate::key_management::{KeyStore, KeyStoreConfig};
    use crate::libp2p::{NetworkMessage, PeerManager};
    use crate::message_pool::{MessagePool, MpoolRpcProvider};
    use crate::networks::ChainConfig;
    use crate::rpc::eth::filter::EthEventHandler;
    use crate::rpc::RPCState;
    use crate::shim::address::Address;
    use crate::state_manager::StateManager;
    use crate::utils::encoding::from_slice_with_fallback;
    use tokio::sync::mpsc;
    use tokio::{sync::RwLock, task::JoinSet};

    const TEST_NET_NAME: &str = "test";

    fn ctx() -> (Arc<RPCState<MemoryDB>>, flume::Receiver<NetworkMessage>) {
        let (network_send, network_rx) = flume::bounded(5);
        let (tipset_send, _) = flume::bounded(5);
        let mut services = JoinSet::new();
        let db = Arc::new(MemoryDB::default());
        let chain_config = Arc::new(ChainConfig::default());
        let sync_config = Arc::new(SyncConfig::default());

        let genesis_header = CachingBlockHeader::new(RawBlockHeader {
            miner_address: Address::new_id(0),
            timestamp: 7777,
            ..Default::default()
        });

        let cs_arc = Arc::new(
            ChainStore::new(
                db.clone(),
                db.clone(),
                db,
                chain_config.clone(),
                genesis_header,
            )
            .unwrap(),
        );

        let state_manager =
            Arc::new(StateManager::new(cs_arc.clone(), chain_config, sync_config).unwrap());
        let state_manager_for_thread = state_manager.clone();
        let cs_for_test = &cs_arc;
        let mpool_network_send = network_send.clone();
        let pool = {
            let bz = hex::decode("904300e80781586082cb7477a801f55c1f2ea5e5d1167661feea60a39f697e1099af132682b81cc5047beacf5b6e80d5f52b9fd90323fb8510a5396416dd076c13c85619e176558582744053a3faef6764829aa02132a1571a76aabdc498a638ea0054d3bb57f41d82015860812d2396cc4592cdf7f829374b01ffd03c5469a4b0a9acc5ccc642797aa0a5498b97b28d90820fedc6f79ff0a6005f5c15dbaca3b8a45720af7ed53000555667207a0ccb50073cd24510995abd4c4e45c1e9e114905018b2da9454190499941e818201582012dd0a6a7d0e222a97926da03adb5a7768d31cc7c5c2bd6828e14a7d25fa3a608182004b76616c69642070726f6f6681d82a5827000171a0e4022030f89a8b0373ad69079dbcbc5addfe9b34dce932189786e50d3eb432ede3ba9c43000f0001d82a5827000171a0e4022052238c7d15c100c1b9ebf849541810c9e3c2d86e826512c6c416d2318fcd496dd82a5827000171a0e40220e5658b3d18cd06e1db9015b4b0ec55c123a24d5be1ea24d83938c5b8397b4f2fd82a5827000171a0e4022018d351341c302a21786b585708c9873565a0d07c42521d4aaf52da3ff6f2e461586102c000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000001a5f2c5439586102b5cd48724dce0fec8799d77fd6c5113276e7f470c8391faa0b5a6033a3eaf357d635705c36abe10309d73592727289680515afd9d424793ba4796b052682d21b03c5c8a37d94827fecc59cdc5750e198fdf20dee012f4d627c6665132298ab95004500053724e0").unwrap();
            let header = from_slice_with_fallback::<CachingBlockHeader>(&bz).unwrap();
            let ts = Tipset::from(header);
            let db = cs_for_test.blockstore();
            let tsk = ts.key();
            cs_for_test
                .set_heaviest_tipset(Arc::new(ts.clone()))
                .unwrap();

            for i in tsk.to_cids() {
                let bz2 = bz.clone();
                db.put_keyed(&i, &bz2).unwrap();
            }

            let provider =
                MpoolRpcProvider::new(cs_arc.publisher().clone(), state_manager_for_thread.clone());
            MessagePool::new(
                provider,
                "test".to_string(),
                mpool_network_send,
                Default::default(),
                state_manager_for_thread.chain_config().clone(),
                &mut services,
            )
            .unwrap()
        };
        let start_time = chrono::Utc::now();

        let peer_manager = Arc::new(PeerManager::default());
        let sync_network_context =
            SyncNetworkContext::new(network_send, peer_manager, state_manager.blockstore_owned());
        let state = Arc::new(RPCState {
            state_manager,
            keystore: Arc::new(RwLock::new(KeyStore::new(KeyStoreConfig::Memory).unwrap())),
            mpool: Arc::new(pool),
            bad_blocks: Default::default(),
            sync_state: Arc::new(parking_lot::RwLock::new(Default::default())),
            eth_event_handler: Arc::new(EthEventHandler::new()),
            sync_network_context,
            network_name: TEST_NET_NAME.to_owned(),
            start_time,
            shutdown: mpsc::channel(1).0, // dummy for tests
            tipset_send,
        });
        (state, network_rx)
    }

    #[tokio::test]
    async fn set_check_bad() {
        let (ctx, _) = ctx();

        let cid = "bafy2bzacea3wsdh6y3a36tb3skempjoxqpuyompjbmfeyf34fi3uy6uue42v4"
            .parse::<Cid>()
            .unwrap();

        let reason = SyncCheckBad::handle(ctx.clone(), (cid,)).await.unwrap();
        assert_eq!(reason, "");

        // Mark that block as bad manually and check again to verify
        SyncMarkBad::handle(ctx.clone(), (cid,)).await.unwrap();

        let reason = SyncCheckBad::handle(ctx.clone(), (cid,)).await.unwrap();
        assert_eq!(reason, "Marked bad manually through RPC API");
    }

    #[tokio::test]
    async fn sync_state_test() {
        let (ctx, _) = ctx();

        let st_copy = ctx.sync_state.clone();

        let ret = SyncState::handle(ctx.clone(), ()).await.unwrap();
        assert_eq!(ret.active_syncs, nonempty![st_copy.as_ref().read().clone()]);

        // update cloned state
        st_copy.write().set_stage(SyncStage::Messages);
        st_copy.write().set_epoch(4);

        let ret = SyncState::handle(ctx.clone(), ()).await.unwrap();

        assert_eq!(ret.active_syncs, nonempty![st_copy.as_ref().read().clone()]);
    }
}
