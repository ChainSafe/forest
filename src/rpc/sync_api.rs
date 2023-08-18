// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
#![allow(clippy::unused_async)]

use crate::chain_sync::SyncState;
use crate::json::cid::CidJson;
use crate::rpc_api::{
    data_types::{RPCState, RPCSyncState},
    sync_api::*,
};
use fvm_ipld_blockstore::Blockstore;
use jsonrpc_v2::{Data, Error as JsonRpcError, Params};
use parking_lot::RwLock;

/// Checks if a given block is marked as bad.
pub(in crate::rpc) async fn sync_check_bad<DB>(
    data: Data<RPCState<DB>>,
    Params(params): Params<SyncCheckBadParams>,
) -> Result<SyncCheckBadResult, JsonRpcError>
where
    DB: Blockstore,
{
    let (CidJson(cid),) = params;
    Ok(data.bad_blocks.peek(&cid).unwrap_or_default())
}

/// Marks a block as bad, meaning it will never be synced.
pub(in crate::rpc) async fn sync_mark_bad<DB>(
    data: Data<RPCState<DB>>,
    Params(params): Params<SyncMarkBadParams>,
) -> Result<SyncMarkBadResult, JsonRpcError>
where
    DB: Blockstore,
{
    let (CidJson(cid),) = params;
    data.bad_blocks
        .put(cid, "Marked bad manually through RPC API".to_string());
    Ok(())
}

async fn clone_state(state: &RwLock<SyncState>) -> SyncState {
    state.read().clone()
}

/// Returns the current status of the `ChainSync` process.
pub(in crate::rpc) async fn sync_state<DB>(
    data: Data<RPCState<DB>>,
) -> Result<SyncStateResult, JsonRpcError>
where
    DB: Blockstore,
{
    let active_syncs = vec![clone_state(data.sync_state.as_ref()).await];
    Ok(RPCSyncState { active_syncs })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::beacon::{mock_beacon::MockBeacon, BeaconPoint, BeaconSchedule};
    use crate::blocks::{BlockHeader, Tipset};
    use crate::chain::ChainStore;
    use crate::chain_sync::SyncStage;
    use crate::db::MemoryDB;
    use crate::key_management::{KeyStore, KeyStoreConfig};
    use crate::libp2p::NetworkMessage;
    use crate::message_pool::{MessagePool, MpoolRpcProvider};
    use crate::networks::ChainConfig;
    use crate::shim::address::Address;
    use crate::state_manager::StateManager;
    use crate::utils::encoding::from_slice_with_fallback;
    use serde_json::from_str;
    use tokio::{sync::RwLock, task::JoinSet};

    use super::*;

    const TEST_NET_NAME: &str = "test";

    fn state_setup() -> (Arc<RPCState<MemoryDB>>, flume::Receiver<NetworkMessage>) {
        let beacon = Arc::new(BeaconSchedule(vec![BeaconPoint {
            height: 0,
            beacon: Box::<MockBeacon>::default(),
        }]));

        let (network_send, network_rx) = flume::bounded(5);
        let mut services = JoinSet::new();
        let db = Arc::new(MemoryDB::default());
        let chain_config = Arc::new(ChainConfig::default());

        let genesis_header = BlockHeader::builder()
            .miner_address(Address::new_id(0))
            .timestamp(7777)
            .build()
            .unwrap();

        let cs_arc = Arc::new(
            ChainStore::new(db.clone(), db, chain_config.clone(), genesis_header).unwrap(),
        );

        let state_manager = Arc::new(StateManager::new(cs_arc.clone(), chain_config).unwrap());
        let state_manager_for_thread = state_manager.clone();
        let cs_for_test = &cs_arc;
        let cs_for_chain = &cs_arc;
        let mpool_network_send = network_send.clone();
        let pool = {
            let bz = hex::decode("904300e80781586082cb7477a801f55c1f2ea5e5d1167661feea60a39f697e1099af132682b81cc5047beacf5b6e80d5f52b9fd90323fb8510a5396416dd076c13c85619e176558582744053a3faef6764829aa02132a1571a76aabdc498a638ea0054d3bb57f41d82015860812d2396cc4592cdf7f829374b01ffd03c5469a4b0a9acc5ccc642797aa0a5498b97b28d90820fedc6f79ff0a6005f5c15dbaca3b8a45720af7ed53000555667207a0ccb50073cd24510995abd4c4e45c1e9e114905018b2da9454190499941e818201582012dd0a6a7d0e222a97926da03adb5a7768d31cc7c5c2bd6828e14a7d25fa3a608182004b76616c69642070726f6f6681d82a5827000171a0e4022030f89a8b0373ad69079dbcbc5addfe9b34dce932189786e50d3eb432ede3ba9c43000f0001d82a5827000171a0e4022052238c7d15c100c1b9ebf849541810c9e3c2d86e826512c6c416d2318fcd496dd82a5827000171a0e40220e5658b3d18cd06e1db9015b4b0ec55c123a24d5be1ea24d83938c5b8397b4f2fd82a5827000171a0e4022018d351341c302a21786b585708c9873565a0d07c42521d4aaf52da3ff6f2e461586102c000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000001a5f2c5439586102b5cd48724dce0fec8799d77fd6c5113276e7f470c8391faa0b5a6033a3eaf357d635705c36abe10309d73592727289680515afd9d424793ba4796b052682d21b03c5c8a37d94827fecc59cdc5750e198fdf20dee012f4d627c6665132298ab95004500053724e0").unwrap();
            let header = from_slice_with_fallback::<BlockHeader>(&bz).unwrap();
            let ts = Tipset::from(header);
            let db = cs_for_test.blockstore();
            let tsk = ts.key();
            cs_for_test
                .set_heaviest_tipset(Arc::new(ts.clone()))
                .unwrap();

            for i in tsk.cids.into_iter() {
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
                state_manager_for_thread.chain_config(),
                &mut services,
            )
            .unwrap()
        };
        let (new_mined_block_tx, _) = flume::bounded(5);
        let start_time = chrono::Utc::now();
        let (gc_event_tx, _) = flume::unbounded();

        let state = Arc::new(RPCState {
            state_manager,
            keystore: Arc::new(RwLock::new(KeyStore::new(KeyStoreConfig::Memory).unwrap())),
            mpool: Arc::new(pool),
            bad_blocks: Default::default(),
            sync_state: Arc::new(parking_lot::RwLock::new(Default::default())),
            network_send,
            network_name: TEST_NET_NAME.to_owned(),
            start_time,
            chain_store: cs_for_chain.clone(),
            beacon,
            new_mined_block_tx,
            gc_event_tx,
        });
        (state, network_rx)
    }

    #[tokio::test]
    async fn set_check_bad() {
        let (state, _) = state_setup();

        let cid: CidJson =
            from_str(r#"{"/":"bafy2bzacea3wsdh6y3a36tb3skempjoxqpuyompjbmfeyf34fi3uy6uue42v4"}"#)
                .unwrap();
        match sync_check_bad(Data(state.clone()), Params((cid.clone(),))).await {
            Ok(reason) => assert_eq!(reason, ""),
            Err(e) => std::panic::panic_any(e),
        }

        // Mark that block as bad manually and check again to verify
        assert!(sync_mark_bad(Data(state.clone()), Params((cid.clone(),)))
            .await
            .is_ok());
        match sync_check_bad(Data(state), Params((cid,))).await {
            Ok(reason) => assert_eq!(reason, "Marked bad manually through RPC API"),
            Err(e) => std::panic::panic_any(e),
        }
    }

    #[tokio::test]
    async fn sync_state_test() {
        let (state, _) = state_setup();

        let st_copy = state.sync_state.clone();

        match sync_state(Data(state.clone())).await {
            Ok(ret) => assert_eq!(ret.active_syncs, vec![clone_state(st_copy.as_ref()).await]),
            Err(e) => std::panic::panic_any(e),
        }

        // update cloned state
        st_copy.write().set_stage(SyncStage::Messages);
        st_copy.write().set_epoch(4);

        match sync_state(Data(state.clone())).await {
            Ok(ret) => {
                assert_ne!(ret.active_syncs, vec![]);
                assert_eq!(ret.active_syncs, vec![clone_state(st_copy.as_ref()).await]);
            }
            Err(e) => std::panic::panic_any(e),
        }
    }
}
