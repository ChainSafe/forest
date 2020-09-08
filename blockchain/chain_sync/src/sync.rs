// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#[cfg(test)]
mod peer_test;

use super::bad_block_cache::BadBlockCache;
use super::bucket::{SyncBucket, SyncBucketSet};
use super::network_handler::NetworkHandler;
use super::peer_manager::PeerManager;
use super::sync_state::SyncState;
use super::sync_worker::SyncWorker;
use super::{Error, SyncNetworkContext};
use amt::Amt;
use async_std::sync::{channel, Receiver, RwLock, Sender};
use async_std::task::JoinHandle;
use beacon::Beacon;
use blocks::{Block, FullTipset, Tipset, TipsetKeys, TxMeta};
use chain::ChainStore;
use cid::{multihash::Blake2b256, Cid};
use encoding::{Cbor, Error as EncodingError};
use flo_stream::{MessagePublisher, Publisher};
use forest_libp2p::{hello::HelloRequest, NetworkEvent, NetworkMessage};
use futures::stream::StreamExt;
use ipld_blockstore::BlockStore;
use libp2p::core::PeerId;
use log::{debug, warn};
use message::{SignedMessage, UnsignedMessage};
use num_traits::Zero;
use state_manager::StateManager;
use std::sync::Arc;

/// Number of tasks spawned for sync workers.
// TODO benchmark and/or add this as a config option.
const WORKER_TASKS: usize = 3;

#[derive(Debug, PartialEq)]
enum ChainSyncState {
    /// Bootstrapping peers before starting sync.
    Bootstrap,
    /// Syncing chain with BlockSync protocol.
    Initial,
    /// Following chain with blocks received over gossipsub.
    _Follow,
}

/// Struct that handles the ChainSync logic. This handles incoming network events such as
/// gossipsub messages, Hello protocol requests, as well as sending and receiving BlockSync
/// messages to be able to do the initial sync.
pub struct ChainSyncer<DB, TBeacon> {
    /// State of general `ChainSync` protocol.
    state: ChainSyncState,

    /// Syncing state of chain sync workers.
    // TODO revisit this type, necessary for two sets of Arc<Mutex<>> because each state is
    // on seperate thread and needs to be mutated independently, but the vec needs to be read
    // on the RPC API thread and mutated on this thread.
    worker_state: Arc<RwLock<Vec<Arc<RwLock<SyncState>>>>>,

    /// Drand randomness beacon
    beacon: Arc<TBeacon>,

    /// manages retrieving and updates state objects
    state_manager: Arc<StateManager<DB>>,

    /// Bucket queue for incoming tipsets
    sync_queue: SyncBucketSet,
    /// Represents tipsets related to ones already being synced to avoid duplicate work.
    active_sync_tipsets: SyncBucketSet,

    /// Represents next tipset to be synced.
    next_sync_target: SyncBucket,

    /// access and store tipsets / blocks / messages
    chain_store: Arc<ChainStore<DB>>,

    /// Context to be able to send requests to p2p network
    network: SyncNetworkContext,

    /// the known genesis tipset
    genesis: Arc<Tipset>,

    /// Bad blocks cache, updates based on invalid state transitions.
    /// Will mark any invalid blocks and all childen as bad in this bounded cache
    bad_blocks: Arc<BadBlockCache>,

    ///  incoming network events to be handled by syncer
    net_handler: NetworkHandler,

    /// Peer manager to handle full peers to send ChainSync requests to
    peer_manager: Arc<PeerManager>,
}

impl<DB, TBeacon> ChainSyncer<DB, TBeacon>
where
    TBeacon: Beacon + Sync + Send + 'static,
    DB: BlockStore + Sync + Send + 'static,
{
    pub fn new(
        chain_store: Arc<ChainStore<DB>>,
        beacon: Arc<TBeacon>,
        network_send: Sender<NetworkMessage>,
        network_rx: Receiver<NetworkEvent>,
        genesis: Arc<Tipset>,
    ) -> Result<Self, Error> {
        let state_manager = Arc::new(StateManager::new(chain_store.db.clone()));

        // Split incoming channel to handle blocksync requests
        let mut event_send = Publisher::new(30);
        let network = SyncNetworkContext::new(network_send, event_send.subscribe());

        let peer_manager = Arc::new(PeerManager::default());

        let net_handler = NetworkHandler::new(network_rx, event_send);

        Ok(Self {
            state: ChainSyncState::Bootstrap,
            worker_state: Default::default(),
            beacon,
            state_manager,
            chain_store,
            network,
            genesis,
            bad_blocks: Arc::new(BadBlockCache::default()),
            net_handler,
            peer_manager,
            sync_queue: SyncBucketSet::default(),
            active_sync_tipsets: SyncBucketSet::default(),
            next_sync_target: SyncBucket::default(),
        })
    }

    /// Returns a clone of the bad blocks cache to be used outside of chain sync.
    pub fn bad_blocks_cloned(&self) -> Arc<BadBlockCache> {
        self.bad_blocks.clone()
    }

    /// Returns the atomic reference to the syncing state.
    pub fn sync_state_cloned(&self) -> Arc<RwLock<Vec<Arc<RwLock<SyncState>>>>> {
        self.worker_state.clone()
    }

    /// Spawns a network handler and begins the syncing process.
    pub async fn start(mut self) {
        self.net_handler.spawn(Arc::clone(&self.peer_manager));
        let (worker_tx, worker_rx) = channel(20);
        for _ in 0..WORKER_TASKS {
            self.spawn_worker(worker_rx.clone()).await;
        }

        // TODO switch worker tx is_empty check to a future to use select! macro
        loop {
            if worker_tx.is_empty() {
                if let Some(ts) = self.next_sync_target.heaviest_tipset() {
                    worker_tx.send(ts).await;
                } else if let Some(ts) = self.sync_queue.heaviest() {
                    worker_tx.send(ts).await;
                }
            }
            if let Some(event) = self.network.receiver.next().await {
                match event {
                    NetworkEvent::HelloRequest { request, channel } => {
                        let source = channel.peer.clone();
                        debug!(
                            "Message inbound, heaviest tipset cid: {:?}",
                            request.heaviest_tip_set
                        );
                        match self
                            .fetch_tipset(
                                source.clone(),
                                &TipsetKeys::new(request.heaviest_tip_set),
                            )
                            .await
                        {
                            Ok(fts) => {
                                if let Err(e) = self.inform_new_head(source.clone(), &fts).await {
                                    warn!("Failed to sync with provided tipset: {}", e);
                                };
                            }
                            Err(e) => {
                                warn!("Failed to fetch full tipset from peer ({}): {}", source, e);
                            }
                        }
                    }
                    NetworkEvent::PeerDialed { peer_id } => {
                        let heaviest = self.chain_store.heaviest_tipset().await.unwrap();
                        self.network
                            .hello_request(
                                peer_id,
                                HelloRequest {
                                    heaviest_tip_set: heaviest.cids().to_vec(),
                                    heaviest_tipset_height: heaviest.epoch(),
                                    heaviest_tipset_weight: heaviest.weight().clone(),
                                    genesis_hash: self.genesis.blocks()[0].cid().clone(),
                                },
                            )
                            .await
                    }
                    _ => (),
                }
            }
        }
    }

    /// Spawns a new sync worker and pushes the state to the `ChainSyncer`
    async fn spawn_worker(&mut self, channel: Receiver<Arc<Tipset>>) -> JoinHandle<()> {
        let state = Arc::new(RwLock::new(SyncState::default()));

        // push state to managed states in Syncer.
        self.worker_state.write().await.push(state.clone());
        SyncWorker {
            state,
            beacon: self.beacon.clone(),
            state_manager: self.state_manager.clone(),
            chain_store: self.chain_store.clone(),
            network: self.network.clone(),
            genesis: self.genesis.clone(),
            bad_blocks: self.bad_blocks.clone(),
            peer_manager: self.peer_manager.clone(),
        }
        .spawn(channel)
        .await
    }

    /// informs the syncer about a new potential tipset
    /// This should be called when connecting to new peers, and additionally
    /// when receiving new blocks from the network
    pub async fn inform_new_head(&mut self, peer: PeerId, fts: &FullTipset) -> Result<(), Error> {
        // check if full block is nil and if so return error
        if fts.blocks().is_empty() {
            return Err(Error::NoBlocks);
        }
        // TODO: Check if tipset has height that is too far ahead to be possible

        for block in fts.blocks() {
            if let Some(bad) = self.bad_blocks.peek(block.cid()).await {
                warn!("Bad block detected, cid: {:?}", bad);
                return Err(Error::Other("Block marked as bad".to_string()));
            }
            // validate message data
            self.validate_msg_meta(block)?;
        }
        // TODO: Publish LocalIncoming blocks

        // compare target_weight to heaviest weight stored; ignore otherwise
        let best_weight = match self.chain_store.heaviest_tipset().await {
            Some(ts) => ts.weight().clone(),
            None => Zero::zero(),
        };
        let target_weight = fts.weight();

        if target_weight.gt(&best_weight) {
            self.set_peer_head(peer, Arc::new(fts.to_tipset())).await;
        }
        // incoming tipset from miners does not appear to be better than our best chain, ignoring for now
        Ok(())
    }

    async fn set_peer_head(&mut self, peer: PeerId, ts: Arc<Tipset>) {
        self.peer_manager
            .add_peer(peer, Some(Arc::clone(&ts)))
            .await;

        // Only update target on initial sync
        if self.state == ChainSyncState::Bootstrap {
            if let Some(best_target) = self.select_sync_target().await {
                self.schedule_tipset(best_target).await;
                self.state = ChainSyncState::Initial;
                return;
            }
        }
        self.schedule_tipset(ts).await;
    }

    /// Selects max sync target from current peer set
    async fn select_sync_target(&self) -> Option<Arc<Tipset>> {
        // Retrieve all peer heads from peer manager
        let heads = self.peer_manager.get_peer_heads().await;
        heads.iter().max_by_key(|h| h.epoch()).cloned()
    }

    /// Schedules a new tipset to be handled by the sync manager
    async fn schedule_tipset(&mut self, tipset: Arc<Tipset>) {
        debug!("Scheduling incoming tipset to sync: {:?}", tipset.cids());

        let mut related_to_active = false;
        for act_state in self.worker_state.read().await.iter() {
            if let Some(target) = act_state.read().await.target() {
                if target == &tipset {
                    return;
                }

                if tipset.parents() == target.key() {
                    related_to_active = true;
                }
            }
        }

        // Check if related to active tipset buckets.
        if !related_to_active && self.active_sync_tipsets.related_to_any(tipset.as_ref()) {
            related_to_active = true;
        }

        if related_to_active {
            self.active_sync_tipsets.insert(tipset);
            return;
        }

        // if next_sync_target is from same chain as incoming tipset add it to be synced next
        if self.next_sync_target.is_same_chain_as(&tipset) {
            self.next_sync_target.add(tipset);
        } else {
            // add incoming tipset to queue to by synced later
            self.sync_queue.insert(tipset);
            // update next sync target if empty
            if self.next_sync_target.is_empty() {
                if let Some(target_bucket) = self.sync_queue.pop() {
                    self.next_sync_target = target_bucket;
                }
            }
        }
    }
    /// Validates message root from header matches message root generated from the
    /// bls and secp messages contained in the passed in block and stores them in a key-value store
    fn validate_msg_meta(&self, block: &Block) -> Result<(), Error> {
        let sm_root = compute_msg_meta(
            self.chain_store.blockstore(),
            block.bls_msgs(),
            block.secp_msgs(),
        )?;
        if block.header().messages() != &sm_root {
            return Err(Error::InvalidRoots);
        }

        self.chain_store.put_messages(block.bls_msgs())?;
        self.chain_store.put_messages(block.secp_msgs())?;

        Ok(())
    }

    /// Returns FullTipset from store if TipsetKeys exist in key-value store otherwise requests FullTipset
    /// from block sync
    async fn fetch_tipset(
        &mut self,
        peer_id: PeerId,
        tsk: &TipsetKeys,
    ) -> Result<FullTipset, String> {
        let fts = match self.load_fts(tsk) {
            Ok(fts) => fts,
            _ => return self.network.blocksync_fts(peer_id, tsk).await,
        };

        Ok(fts)
    }
    /// Returns a reconstructed FullTipset from store if keys exist
    fn load_fts(&self, keys: &TipsetKeys) -> Result<FullTipset, Error> {
        let mut blocks = Vec::new();
        // retrieve tipset from store based on passed in TipsetKeys
        let ts = self.chain_store.tipset_from_keys(keys)?;
        for header in ts.blocks() {
            // retrieve bls and secp messages from specified BlockHeader
            let (bls_msgs, secp_msgs) =
                chain::block_messages(self.chain_store.blockstore(), &header)?;

            // construct a full block
            let full_block = Block {
                header: header.clone(),
                bls_messages: bls_msgs,
                secp_messages: secp_msgs,
            };
            // push vector of full blocks to build FullTipset
            blocks.push(full_block);
        }
        // construct FullTipset
        let fts = FullTipset::new(blocks)?;
        Ok(fts)
    }
}

/// Returns message root CID from bls and secp message contained in the param Block
fn compute_msg_meta<DB: BlockStore>(
    blockstore: &DB,
    bls_msgs: &[UnsignedMessage],
    secp_msgs: &[SignedMessage],
) -> Result<Cid, Error> {
    // collect bls and secp cids
    let bls_cids = cids_from_messages(bls_msgs)?;
    let secp_cids = cids_from_messages(secp_msgs)?;

    // generate Amt and batch set message values
    let bls_root = Amt::new_from_slice(blockstore, &bls_cids)?;
    let secp_root = Amt::new_from_slice(blockstore, &secp_cids)?;

    let meta = TxMeta {
        bls_message_root: bls_root,
        secp_message_root: secp_root,
    };

    // store message roots and receive meta_root cid
    let meta_root = blockstore
        .put(&meta, Blake2b256)
        .map_err(|e| Error::Other(e.to_string()))?;

    Ok(meta_root)
}

fn cids_from_messages<T: Cbor>(messages: &[T]) -> Result<Vec<Cid>, EncodingError> {
    messages.iter().map(Cbor::cid).collect()
}

// TODO use tests
// #[cfg(test)]
// mod tests {
//     use super::*;
//     use async_std::sync::channel;
//     use async_std::sync::Sender;
//     use beacon::MockBeacon;
//     use blocks::BlockHeader;
//     use db::MemoryDB;
//     use forest_libp2p::NetworkEvent;
//     use std::sync::Arc;
//     use test_utils::{construct_blocksync_response, construct_messages, construct_tipset};

//     fn chain_syncer_setup(
//         db: Arc<MemoryDB>,
//     ) -> (
//         ChainSyncer<MemoryDB, MockBeacon>,
//         Sender<NetworkEvent>,
//         Receiver<NetworkMessage>,
//     ) {
//         let chain_store = Arc::new(ChainStore::new(db));

//         let (local_sender, test_receiver) = channel(20);
//         let (event_sender, event_receiver) = channel(20);

//         let gen = dummy_header();
//         chain_store.set_genesis(gen.clone()).unwrap();

//         let beacon = Arc::new(MockBeacon::new(Duration::from_secs(1)));

//         let genesis_ts = Arc::new(Tipset::new(vec![gen]).unwrap());
//         (
//             ChainSyncer::new(
//                 chain_store,
//                 beacon,
//                 local_sender,
//                 event_receiver,
//                 genesis_ts,
//             )
//             .unwrap(),
//             event_sender,
//             test_receiver,
//         )
//     }

//     fn send_blocksync_response(blocksync_message: Receiver<NetworkMessage>) {
//         let rpc_response = construct_blocksync_response();

//         task::block_on(async {
//             match blocksync_message.recv().await.unwrap() {
//                 NetworkMessage::BlockSyncRequest {
//                     peer_id: _,
//                     request: _,
//                     response_channel,
//                 } => {
//                     response_channel.send(rpc_response).unwrap();
//                 }
//                 _ => unreachable!(),
//             }
//         });
//     }

//     fn dummy_header() -> BlockHeader {
//         BlockHeader::builder()
//             .miner_address(Address::new_id(1000))
//             .messages(Cid::new_from_cbor(&[1, 2, 3], Blake2b256))
//             .message_receipts(Cid::new_from_cbor(&[1, 2, 3], Blake2b256))
//             .state_root(Cid::new_from_cbor(&[1, 2, 3], Blake2b256))
//             .build()
//             .unwrap()
//     }
//     #[test]
//     fn chainsync_constructor() {
//         let db = Arc::new(MemoryDB::default());

//         // Test just makes sure that the chain syncer can be created without using a live database or
//         // p2p network (local channels to simulate network messages and responses)
//         let _chain_syncer = chain_syncer_setup(db);
//     }

//     #[test]
//     fn sync_headers_reverse_given_tipsets_test() {
//         let db = Arc::new(MemoryDB::default());
//         let (mut cs, _event_sender, network_receiver) = chain_syncer_setup(db);

//         cs.net_handler.spawn(Arc::clone(&cs.peer_manager));

//         // params for sync_headers_reverse
//         let source = PeerId::random();
//         let head = construct_tipset(4, 10);
//         let to = construct_tipset(1, 10);

//         task::block_on(async move {
//             cs.peer_manager.add_peer(source.clone(), None).await;
//             assert_eq!(cs.peer_manager.len().await, 1);
//             // make blocksync request
//             let return_set = task::spawn(async move { cs.sync_headers_reverse(head, &to).await });
//             // send blocksync response to channel
//             send_blocksync_response(network_receiver);
//             assert_eq!(return_set.await.unwrap().len(), 4);
//         });
//     }

//     #[test]
//     fn compute_msg_meta_given_msgs_test() {
//         let db = Arc::new(MemoryDB::default());
//         let (cs, _, _) = chain_syncer_setup(db);

//         let (bls, secp) = construct_messages();

//         let expected_root =
//             Cid::from_raw_cid("bafy2bzaceasssikoiintnok7f3sgnekfifarzobyr3r4f25sgxmn23q4c35ic")
//                 .unwrap();

//         let root = compute_msg_meta(cs.chain_store.blockstore(), &[bls], &[secp]).unwrap();
//         assert_eq!(root, expected_root);
//     }

//     #[test]
//     fn empty_msg_meta_vector() {
//         let blockstore = MemoryDB::default();
//         let usm: Vec<UnsignedMessage> =
//             encoding::from_slice(&base64::decode("gA==").unwrap()).unwrap();
//         let sm: Vec<SignedMessage> =
//             encoding::from_slice(&base64::decode("gA==").unwrap()).unwrap();

//         assert_eq!(
//             compute_msg_meta(&blockstore, &usm, &sm)
//                 .unwrap()
//                 .to_string(),
//             "bafy2bzacecmda75ovposbdateg7eyhwij65zklgyijgcjwynlklmqazpwlhba"
//         );
//     }
// }
