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
    use chain_sync::SyncStage;
    use db::MemoryDB;
    use forest_libp2p::NetworkMessage;
    use futures::StreamExt;
    use serde_json::from_str;
    use std::sync::Arc;
    use wallet::MemKeyStore;

    const TEST_NET_NAME: &str = "test";

    fn state_setup() -> (
        Arc<RpcState<MemoryDB, MemKeyStore>>,
        Receiver<NetworkMessage>,
    ) {
        let (network_send, network_rx) = channel(5);
        let state = Arc::new(RpcState {
            store: Arc::new(MemoryDB::default()),
            keystore: Arc::new(RwLock::new(wallet::MemKeyStore::new())),
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

        let block_json: GossipBlockJson = from_str(r#"{"Header":{"Miner":"t01234","Ticket":{"VRFProof":"Ynl0ZSBhcnJheQ=="},"ElectionProof":{"VRFProof":"Ynl0ZSBhcnJheQ=="},"BeaconEntries":null,"WinPoStProof":null,"Parents":null,"ParentWeight":"0","Height":10101,"ParentStateRoot":{"/":"bafy2bzacea3wsdh6y3a36tb3skempjoxqpuyompjbmfeyf34fi3uy6uue42v4"},"ParentMessageReceipts":{"/":"bafy2bzacea3wsdh6y3a36tb3skempjoxqpuyompjbmfeyf34fi3uy6uue42v4"},"Messages":{"/":"bafy2bzacea3wsdh6y3a36tb3skempjoxqpuyompjbmfeyf34fi3uy6uue42v4"},"BLSAggregate":{"Type":2,"Data":"Ynl0ZSBhcnJheQ=="},"Timestamp":42,"BlockSig":{"Type":2,"Data":"Ynl0ZSBhcnJheQ=="},"ForkSignaling":42},"BlsMessages":null,"SecpkMessages":null}"#).unwrap();

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
