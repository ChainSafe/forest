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
use wallet::KeyStore;

#[derive(Serialize)]
pub struct RPCSyncState {
    #[serde(rename = "ActiveSyncs")]
    active_syncs: Vec<SyncState>,
}

/// Checks if a given block is marked as bad.
pub(crate) async fn sync_check_bad<DB, KS>(
    data: Data<RpcState<DB, KS>>,
    Params(params): Params<(CidJson,)>,
) -> Result<String, JsonRpcError>
where
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
{
    let (CidJson(cid),) = params;
    Ok(data.bad_blocks.peek(&cid).await.unwrap_or_default())
}

/// Marks a block as bad, meaning it will never be synced.
pub(crate) async fn sync_mark_bad<DB, KS>(
    data: Data<RpcState<DB, KS>>,
    Params(params): Params<(CidJson,)>,
) -> Result<(), JsonRpcError>
where
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
{
    let (CidJson(cid),) = params;
    data.bad_blocks
        .put(cid, "Marked bad manually through RPC API".to_string())
        .await;
    Ok(())
}

// TODO SyncIncomingBlocks (requires websockets)

/// Returns the current status of the ChainSync process.
pub(crate) async fn sync_state<DB, KS>(
    data: Data<RpcState<DB, KS>>,
) -> Result<RPCSyncState, JsonRpcError>
where
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
{
    let state = data.sync_state.read().await.clone();
    Ok(RPCSyncState {
        active_syncs: vec![state],
    })
}

/// Submits block to be sent through gossipsub.
pub(crate) async fn sync_submit_block<DB, KS>(
    data: Data<RpcState<DB, KS>>,
    Params((GossipBlockJson(blk),)): Params<(GossipBlockJson,)>,
) -> Result<(), JsonRpcError>
where
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
{
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

#[cfg(test)]
mod tests {
    use super::*;
    use async_std::sync::{channel, Receiver, RwLock};
    use async_std::task;
    use blocks::{BlockHeader, Tipset};
    use chain::ChainStore;
    use chain_sync::SyncStage;
    use db::{MemoryDB, Store};
    use forest_libp2p::NetworkMessage;
    use futures::StreamExt;
    use message_pool::{MessagePool, MpoolRpcProvider};
    use serde_json::from_str;
    use state_manager::StateManager;
    use std::sync::Arc;
    use wallet::MemKeyStore;

    const TEST_NET_NAME: &str = "test";

    fn state_setup() -> (
        Arc<RpcState<MemoryDB, MemKeyStore>>,
        Receiver<NetworkMessage>,
    ) {
        let (network_send, network_rx) = channel(5);
        let db = Arc::new(MemoryDB::default());
        let state_manager = Arc::new(StateManager::new(db.clone()));

        let pool = task::block_on(async {
            let mut cs = ChainStore::new(db.clone());
            let bz = hex::decode("904300e80781586082cb7477a801f55c1f2ea5e5d1167661feea60a39f697e1099af132682b81cc5047beacf5b6e80d5f52b9fd90323fb8510a5396416dd076c13c85619e176558582744053a3faef6764829aa02132a1571a76aabdc498a638ea0054d3bb57f41d82015860812d2396cc4592cdf7f829374b01ffd03c5469a4b0a9acc5ccc642797aa0a5498b97b28d90820fedc6f79ff0a6005f5c15dbaca3b8a45720af7ed53000555667207a0ccb50073cd24510995abd4c4e45c1e9e114905018b2da9454190499941e818201582012dd0a6a7d0e222a97926da03adb5a7768d31cc7c5c2bd6828e14a7d25fa3a608182004b76616c69642070726f6f6681d82a5827000171a0e4022030f89a8b0373ad69079dbcbc5addfe9b34dce932189786e50d3eb432ede3ba9c43000f0001d82a5827000171a0e4022052238c7d15c100c1b9ebf849541810c9e3c2d86e826512c6c416d2318fcd496dd82a5827000171a0e40220e5658b3d18cd06e1db9015b4b0ec55c123a24d5be1ea24d83938c5b8397b4f2fd82a5827000171a0e4022018d351341c302a21786b585708c9873565a0d07c42521d4aaf52da3ff6f2e461586102c000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000001a5f2c5439586102b5cd48724dce0fec8799d77fd6c5113276e7f470c8391faa0b5a6033a3eaf357d635705c36abe10309d73592727289680515afd9d424793ba4796b052682d21b03c5c8a37d94827fecc59cdc5750e198fdf20dee012f4d627c6665132298ab95004500053724e0").unwrap();
            let header = BlockHeader::unmarshal_cbor(&bz).unwrap();
            let ts = Tipset::new(vec![header]).unwrap();
            let subscriber = cs.subscribe();
            let db = cs.db.clone();
            let tsk = ts.key().cids.clone();
            cs.set_heaviest_tipset(Arc::new(ts)).await.unwrap();

            for i in tsk {
                let bz2 = bz.clone();
                db.as_ref().write(i.key(), bz2).unwrap();
            }
            let provider = MpoolRpcProvider::new(subscriber, state_manager.clone());
            MessagePool::new(provider, "test".to_string())
                .await
                .unwrap()
        });

        let state = Arc::new(RpcState {
            state_manager: state_manager,
            keystore: Arc::new(RwLock::new(wallet::MemKeyStore::new())),
            mpool: Arc::new(pool),
            bad_blocks: Default::default(),
            sync_state: Default::default(),
            network_send,
            network_name: TEST_NET_NAME.to_owned(),
        });
        (state, network_rx)
    }

    #[async_std::test]
    async fn set_check_bad() {
        let (state, _) = state_setup();

        let cid: CidJson =
            from_str(r#"{"/":"bafy2bzacea3wsdh6y3a36tb3skempjoxqpuyompjbmfeyf34fi3uy6uue42v4"}"#)
                .unwrap();
        match sync_check_bad(Data(state.clone()), Params((cid.clone(),))).await {
            Ok(reason) => assert_eq!(reason, ""),
            Err(e) => panic!(e),
        }

        // Mark that block as bad manually and check again to verify
        assert!(sync_mark_bad(Data(state.clone()), Params((cid.clone(),)))
            .await
            .is_ok());
        match sync_check_bad(Data(state), Params((cid,))).await {
            Ok(reason) => assert_eq!(reason, "Marked bad manually through RPC API"),
            Err(e) => panic!(e),
        }
    }

    #[async_std::test]
    async fn sync_state_test() {
        let (state, _) = state_setup();

        let cloned_state = state.sync_state.clone();

        match sync_state(Data(state.clone())).await {
            // TODO this will probably have to be updated when sync state is updated
            Ok(ret) => assert_eq!(ret.active_syncs, vec![cloned_state.read().await.clone()]),
            Err(e) => panic!(e),
        }

        // update cloned state
        cloned_state.write().await.set_stage(SyncStage::Messages);
        cloned_state.write().await.set_epoch(4);

        match sync_state(Data(state.clone())).await {
            Ok(ret) => {
                assert_ne!(ret.active_syncs, vec![Default::default()]);
                assert_eq!(ret.active_syncs, vec![cloned_state.read().await.clone()]);
            }
            Err(e) => panic!(e),
        }
    }

    #[async_std::test]
    async fn sync_submit_test() {
        let (state, mut rx) = state_setup();

        let block_json: GossipBlockJson = from_str(r#"{"Header":{"Miner":"t01234","Ticket":{"VRFProof":"Ynl0ZSBhcnJheQ=="},"ElectionProof":{"WinCount":0,"VRFProof":"Ynl0ZSBhcnJheQ=="},"BeaconEntries":null,"WinPoStProof":null,"Parents":null,"ParentWeight":"0","Height":10101,"ParentStateRoot":{"/":"bafy2bzacea3wsdh6y3a36tb3skempjoxqpuyompjbmfeyf34fi3uy6uue42v4"},"ParentMessageReceipts":{"/":"bafy2bzacea3wsdh6y3a36tb3skempjoxqpuyompjbmfeyf34fi3uy6uue42v4"},"Messages":{"/":"bafy2bzacea3wsdh6y3a36tb3skempjoxqpuyompjbmfeyf34fi3uy6uue42v4"},"BLSAggregate":{"Type":2,"Data":"Ynl0ZSBhcnJheQ=="},"Timestamp":42,"BlockSig":{"Type":2,"Data":"Ynl0ZSBhcnJheQ=="},"ForkSignaling":42,"ParentBaseFee":"1"},"BlsMessages":null,"SecpkMessages":null}"#).unwrap();

        let block_cbor = block_json.0.marshal_cbor().unwrap();

        assert!(sync_submit_block(Data(state), Params((block_json,)))
            .await
            .is_ok());

        let net_msg = rx.next().await.expect("Channel can't be dropped here");
        if let NetworkMessage::PubsubMessage { topic, message } = net_msg {
            assert_eq!(
                topic.to_string(),
                format!("{}/{}", PUBSUB_BLOCK_STR, TEST_NET_NAME)
            );
            assert_eq!(message, block_cbor);
        } else {
            panic!("Unexpected network messages: {:?}", net_msg);
        }
    }
}
