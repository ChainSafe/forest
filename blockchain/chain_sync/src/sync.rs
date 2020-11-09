// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#[cfg(test)]
mod peer_test;

use super::bad_block_cache::BadBlockCache;
use super::bucket::{SyncBucket, SyncBucketSet};
use super::sync_state::SyncState;
use super::sync_worker::SyncWorker;
use super::{Error, SyncNetworkContext};
use amt::Amt;
use async_std::sync::{channel, Receiver, RwLock, Sender};
use async_std::task::{self, JoinHandle};
use beacon::Beacon;
use blocks::{Block, FullTipset, Tipset, TipsetKeys, TxMeta};
use chain::ChainStore;
use cid::{multihash::Blake2b256, Cid};
use encoding::{Cbor, Error as EncodingError};
use fil_types::verifier::ProofVerifier;
use forest_libp2p::{hello::HelloRequest, NetworkEvent, NetworkMessage};
use futures::future::try_join_all;
use futures::select;
use futures::stream::StreamExt;
use ipld_blockstore::BlockStore;
use libp2p::core::PeerId;
use log::{debug, info, trace, warn};
use message::{SignedMessage, UnsignedMessage};
use message_pool::{MessagePool, Provider};
use state_manager::StateManager;
use std::marker::PhantomData;
use std::sync::Arc;

// TODO revisit this type, necessary for two sets of Arc<Mutex<>> because each state is
// on separate thread and needs to be mutated independently, but the vec needs to be read
// on the RPC API thread and mutated on this thread.
type WorkerState = Arc<RwLock<Vec<Arc<RwLock<SyncState>>>>>;

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
pub struct ChainSyncer<DB, TBeacon, V, M> {
    /// State of general `ChainSync` protocol.
    state: ChainSyncState,

    /// Syncing state of chain sync workers.
    worker_state: WorkerState,

    /// Drand randomness beacon
    beacon: Arc<TBeacon>,

    /// manages retrieving and updates state objects
    state_manager: Arc<StateManager<DB>>,

    /// Bucket queue for incoming tipsets
    sync_queue: SyncBucketSet,
    /// Represents tipsets related to ones already being synced to avoid duplicate work.
    active_sync_tipsets: SyncBucketSet,

    /// Represents next tipset to be synced.
    next_sync_target: Option<SyncBucket>,

    /// Context to be able to send requests to p2p network
    network: SyncNetworkContext<DB>,

    /// the known genesis tipset
    genesis: Arc<Tipset>,

    /// Bad blocks cache, updates based on invalid state transitions.
    /// Will mark any invalid blocks and all childen as bad in this bounded cache
    bad_blocks: Arc<BadBlockCache>,

    ///  incoming network events to be handled by syncer
    net_handler: Receiver<NetworkEvent>,

    /// Proof verification implementation.
    verifier: PhantomData<V>,

    mpool: Arc<MessagePool<M>>,
}

impl<DB, TBeacon, V, M> ChainSyncer<DB, TBeacon, V, M>
where
    TBeacon: Beacon + Sync + Send + 'static,
    DB: BlockStore + Sync + Send + 'static,
    V: ProofVerifier + Sync + Send + 'static,
    M: Provider + Sync + Send + 'static,
{
    pub fn new(
        state_manager: Arc<StateManager<DB>>,
        beacon: Arc<TBeacon>,
        mpool: Arc<MessagePool<M>>,
        network_send: Sender<NetworkMessage>,
        network_rx: Receiver<NetworkEvent>,
        genesis: Arc<Tipset>,
    ) -> Result<Self, Error> {
        let network = SyncNetworkContext::new(
            network_send,
            Default::default(),
            state_manager.blockstore_cloned(),
        );

        Ok(Self {
            state: ChainSyncState::Bootstrap,
            worker_state: Default::default(),
            beacon,
            network,
            genesis,
            state_manager,
            bad_blocks: Arc::new(BadBlockCache::default()),
            net_handler: network_rx,
            sync_queue: SyncBucketSet::default(),
            active_sync_tipsets: SyncBucketSet::default(),
            next_sync_target: None,
            verifier: Default::default(),
            mpool,
        })
    }

    /// Returns a clone of the bad blocks cache to be used outside of chain sync.
    pub fn bad_blocks_cloned(&self) -> Arc<BadBlockCache> {
        self.bad_blocks.clone()
    }

    /// Returns a cloned `Arc` of the sync worker state.
    pub fn sync_state_cloned(&self) -> WorkerState {
        self.worker_state.clone()
    }

    /// Spawns a network handler and begins the syncing process.
    pub async fn start(mut self, num_workers: usize) {
        let (worker_tx, worker_rx) = channel(5);
        for _ in 0..num_workers {
            self.spawn_worker(worker_rx.clone()).await;
        }

        // Channels to handle fetching hello tipsets in separate task and return tipset.
        let (new_ts_tx, new_ts_rx) = channel(10);

        let mut fused_handler = self.net_handler.clone().fuse();
        let mut fused_inform_channel = new_ts_rx.fuse();

        loop {
            // TODO would be ideal if this is a future attached to the select
            if worker_tx.is_empty() {
                if let Some(tar) = self.next_sync_target.take() {
                    if let Some(ts) = tar.heaviest_tipset() {
                        self.active_sync_tipsets.insert(ts.clone());
                        worker_tx.send(ts).await;
                    }
                }
            }
            select! {
                network_event = fused_handler.next() => match network_event {
                    Some(NetworkEvent::HelloRequest { request, channel }) => {
                        let source = channel.peer.clone();
                        self.network.peer_manager().update_peer_head(source.clone(), None).await;
                        debug!(
                            "Message inbound, heaviest tipset cid: {:?}",
                            request.heaviest_tip_set
                        );
                        let new_ts_tx_cloned = new_ts_tx.clone();
                        let cs_cloned = self.state_manager.chain_store().clone();
                        let net_cloned = self.network.clone();
                        // TODO determine if tasks started to fetch and load tipsets should be
                        // limited. Currently no cap on this.
                        task::spawn(async {
                            Self::fetch_and_inform_tipset(
                                cs_cloned,
                                net_cloned,
                                source,
                                TipsetKeys::new(request.heaviest_tip_set),
                                new_ts_tx_cloned,
                            )
                            .await;
                        });
                    }
                    Some(NetworkEvent::PeerDialed { peer_id }) => {
                        let heaviest = self.state_manager.chain_store().heaviest_tipset().await.unwrap();
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
                    Some(NetworkEvent::PubsubMessage { source, message }) => {
                        match message {
                            forest_libp2p::PubsubMessage::Block(b) => {
                                let source = match source.clone() {
                                    Some(source) => source,
                                    None => {
                                        warn!("Got a GossipBlock with no Source sender. This should not happen based on Filecoin's GossipSub options");
                                        continue;
                                    }
                                };
                                info!("Received block over GossipSub: {} from {}", b.header.epoch(), source);
                                // Get bls_messages in the store or over Bitswap
                                let bmsgs: Vec<_> = b.bls_messages.into_iter().map(|m| self.network.bitswap_get::<UnsignedMessage>(m)).collect();
                                let bmsgs = try_join_all(bmsgs).await;
                                if let Err(e) = &bmsgs {
                                    warn!("Failed to get UnsignedMessage: {}", e);
                                    continue;
                                }
                                // Get secp_messages in the store or over Bitswap
                                let smsgs: Vec<_> = b.secpk_messages.into_iter().map(|m| self.network.bitswap_get::<SignedMessage>(m)).collect();
                                let smsgs = try_join_all(smsgs).await;
                                if let Err(e) = &smsgs {
                                    warn!("Failed to get SignedMessage: {}", e);
                                    continue;
                                }
                                // Form block
                                let block = Block {
                                    header: b.header,
                                    bls_messages: bmsgs.unwrap(),
                                    secp_messages: smsgs.unwrap(),
                                };
                                let ts = FullTipset::new(vec![block]).unwrap();
                                if let Err(e) = self.inform_new_head(source.clone(), &ts).await {
                                    warn!("failed to inform new head from peer {}", source);
                                }
                            }
                            forest_libp2p::PubsubMessage::Message(m) => {
                                // add message to message pool
                                // TODO handle adding message to mempool in seperate task.
                                if let Err(e) = self.mpool.add(m).await {
                                    trace!("Gossip Message failed to be added to Message pool: {}", e);
                                }
                            }
                        }
                    }
                    // All other network events are being ignored currently
                    _ => (),
                    None => break,
                },
                inform_head_event = fused_inform_channel.next() => match inform_head_event {
                    Some((peer, new_head)) => {
                        if let Err(e) = self.inform_new_head(peer.clone(), &new_head).await {
                            warn!("failed to inform new head from peer {}", peer);
                        }
                    }
                    None => break,
                }
            }
        }
    }

    /// Fetches a tipset from store or network, then passes the tipset back through the channel
    /// to inform of the new head.
    async fn fetch_and_inform_tipset(
        cs: Arc<ChainStore<DB>>,
        network: SyncNetworkContext<DB>,
        peer_id: PeerId,
        tsk: TipsetKeys,
        channel: Sender<(PeerId, FullTipset)>,
    ) {
        match Self::fetch_full_tipset(cs.as_ref(), &network, peer_id.clone(), &tsk).await {
            Ok(fts) => {
                channel.send((peer_id, fts)).await;
            }
            Err(e) => {
                debug!("Failed to fetch full tipset from peer ({}): {}", peer_id, e);
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
            network: self.network.clone(),
            genesis: self.genesis.clone(),
            bad_blocks: self.bad_blocks.clone(),
            verifier: PhantomData::<V>::default(),
        }
        .spawn(channel)
        .await
    }

    /// informs the syncer about a new potential tipset
    /// This should be called when connecting to new peers, and additionally
    /// when receiving new blocks from the network
    pub async fn inform_new_head(&mut self, peer: PeerId, ts: &FullTipset) -> Result<(), Error> {
        // check if full block is nil and if so return error
        if ts.blocks().is_empty() {
            return Err(Error::NoBlocks);
        }
        // TODO: Check if tipset has height that is too far ahead to be possible

        for block in ts.blocks() {
            if let Some(bad) = self.bad_blocks.peek(block.cid()).await {
                warn!("Bad block detected, cid: {:?}", bad);
                return Err(Error::Other("Block marked as bad".to_string()));
            }
        }

        // compare target_weight to heaviest weight stored; ignore otherwise
        let candidate_ts = self
            .state_manager
            .chain_store()
            .heaviest_tipset()
            .await
            // TODO we should be able to queue a tipset with the same weight on a different chain.
            // Currently needed to go GT because equal tipsets are attempted to be synced.
            .map(|heaviest| ts.weight() > heaviest.weight())
            .unwrap_or(true);
        if candidate_ts {
            // Check message meta after all other checks (expensive)
            for block in ts.blocks() {
                self.validate_msg_meta(block)?;
            }
            self.set_peer_head(peer, Arc::new(ts.to_tipset())).await;
        }

        Ok(())
    }

    async fn set_peer_head(&mut self, peer: PeerId, ts: Arc<Tipset>) {
        self.network
            .peer_manager()
            .update_peer_head(peer, Some(Arc::clone(&ts)))
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
        let heads = self.network.peer_manager().get_peer_heads().await;
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
        if let Some(tar) = &mut self.next_sync_target {
            if tar.is_same_chain_as(&tipset) {
                tar.add(tipset);
            }
        } else {
            // add incoming tipset to queue to by synced later
            self.sync_queue.insert(tipset);
            // update next sync target if none
            if self.next_sync_target.is_none() {
                if let Some(target_bucket) = self.sync_queue.pop() {
                    self.next_sync_target = Some(target_bucket);
                }
            }
        }
    }
    /// Validates message root from header matches message root generated from the
    /// bls and secp messages contained in the passed in block and stores them in a key-value store
    fn validate_msg_meta(&self, block: &Block) -> Result<(), Error> {
        let sm_root = compute_msg_meta(
            self.state_manager.blockstore(),
            block.bls_msgs(),
            block.secp_msgs(),
        )?;
        if block.header().messages() != &sm_root {
            return Err(Error::InvalidRoots);
        }

        chain::persist_objects(self.state_manager.blockstore(), block.bls_msgs())?;
        chain::persist_objects(self.state_manager.blockstore(), block.secp_msgs())?;

        Ok(())
    }

    /// Returns `FullTipset` from store if `TipsetKeys` exist in key-value store otherwise requests
    /// `FullTipset` from block sync
    async fn fetch_full_tipset(
        cs: &ChainStore<DB>,
        network: &SyncNetworkContext<DB>,
        peer_id: PeerId,
        tsk: &TipsetKeys,
    ) -> Result<FullTipset, String> {
        let fts = match Self::load_fts(cs, tsk) {
            Ok(fts) => fts,
            Err(_) => network.blocksync_fts(Some(peer_id), tsk).await?,
        };

        Ok(fts)
    }

    /// Returns a reconstructed FullTipset from store if keys exist
    fn load_fts(cs: &ChainStore<DB>, keys: &TipsetKeys) -> Result<FullTipset, Error> {
        let mut blocks = Vec::new();
        // retrieve tipset from store based on passed in TipsetKeys
        let ts = cs.tipset_from_keys(keys)?;
        for header in ts.blocks() {
            // retrieve bls and secp messages from specified BlockHeader
            let (bls_msgs, secp_msgs) = chain::block_messages(cs.blockstore(), &header)?;

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

#[cfg(test)]
mod tests {
    use super::*;
    use async_std::sync::channel;
    use async_std::sync::Sender;
    use async_std::task;
    use beacon::MockBeacon;
    use db::MemoryDB;
    use fil_types::verifier::MockVerifier;
    use forest_libp2p::NetworkEvent;
    use message_pool::{test_provider::TestApi, MessagePool};
    use state_manager::StateManager;
    use std::sync::Arc;
    use std::time::Duration;
    use test_utils::{construct_dummy_header, construct_messages};

    fn chain_syncer_setup(
        db: Arc<MemoryDB>,
    ) -> (
        ChainSyncer<MemoryDB, MockBeacon, MockVerifier, TestApi>,
        Sender<NetworkEvent>,
        Receiver<NetworkMessage>,
    ) {
        let chain_store = Arc::new(ChainStore::new(db.clone()));
        let test_provider = TestApi::default();
        let mpool = task::block_on(MessagePool::new(
            test_provider,
            "test".to_string(),
            Default::default(),
        ))
        .unwrap();
        let mpool = Arc::new(mpool);
        let (local_sender, test_receiver) = channel(20);
        let (event_sender, event_receiver) = channel(20);

        let gen = construct_dummy_header();
        chain_store.set_genesis(&gen).unwrap();

        let beacon = Arc::new(MockBeacon::new(Duration::from_secs(1)));

        let genesis_ts = Arc::new(Tipset::new(vec![gen]).unwrap());
        (
            ChainSyncer::new(
                Arc::new(StateManager::new(chain_store)),
                beacon,
                mpool,
                local_sender,
                event_receiver,
                genesis_ts,
            )
            .unwrap(),
            event_sender,
            test_receiver,
        )
    }

    #[test]
    fn chainsync_constructor() {
        let db = Arc::new(MemoryDB::default());

        // Test just makes sure that the chain syncer can be created without using a live database or
        // p2p network (local channels to simulate network messages and responses)
        let _chain_syncer = chain_syncer_setup(db);
    }

    #[test]
    fn compute_msg_meta_given_msgs_test() {
        let db = Arc::new(MemoryDB::default());
        let (cs, _, _) = chain_syncer_setup(db);

        let (bls, secp) = construct_messages();

        let expected_root =
            Cid::from_raw_cid("bafy2bzaceasssikoiintnok7f3sgnekfifarzobyr3r4f25sgxmn23q4c35ic")
                .unwrap();

        let root = compute_msg_meta(cs.state_manager.blockstore(), &[bls], &[secp]).unwrap();
        assert_eq!(root, expected_root);
    }

    #[test]
    fn empty_msg_meta_vector() {
        let blockstore = MemoryDB::default();
        let usm: Vec<UnsignedMessage> =
            encoding::from_slice(&base64::decode("gA==").unwrap()).unwrap();
        let sm: Vec<SignedMessage> =
            encoding::from_slice(&base64::decode("gA==").unwrap()).unwrap();

        assert_eq!(
            compute_msg_meta(&blockstore, &usm, &sm)
                .unwrap()
                .to_string(),
            "bafy2bzacecmda75ovposbdateg7eyhwij65zklgyijgcjwynlklmqazpwlhba"
        );
    }
}
