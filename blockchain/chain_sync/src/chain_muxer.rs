#[cfg(test)]
mod peer_test;

use crate::bad_block_cache::BadBlockCache;
use crate::bucket::{SyncBucket, SyncBucketSet};
use crate::network_context::HelloResponseFuture;
use crate::sync_state::SyncState;
// use crate::sync_worker::SyncWorker;
use crate::tipset_syncer::{TipsetProcessor, TipsetRangeSyncer};
use crate::validation::TipsetValidator;
use crate::{network_context::SyncNetworkContext, Error};

use amt::Amt;
use beacon::{Beacon, BeaconSchedule};
use blocks::{Block, FullTipset, GossipBlock, Tipset, TipsetKeys, TxMeta};
use chain::ChainStore;
use cid::{Cid, Code::Blake2b256};
use clock::ChainEpoch;
use encoding::{Cbor, Error as EncodingError};
use fil_types::verifier::ProofVerifier;
use forest_libp2p::{
    hello::HelloRequest, rpc::RequestResponseError, NetworkEvent, NetworkMessage, PubsubMessage,
};
use ipld_blockstore::BlockStore;
use libp2p::core::PeerId;
use message::{SignedMessage, UnsignedMessage};
use message_pool::{MessagePool, Provider};
use networks::BLOCK_DELAY_SECS;
use state_manager::StateManager;

use async_std::channel::{Receiver, Sender};
use async_std::pin::Pin;
use async_std::stream::StreamExt;
use async_std::sync::{Mutex, RwLock};
use async_std::task::{self, Context, JoinHandle, Poll};
use futures::{
    future,
    future::try_join_all,
    future::{Future, FutureExt},
    stream::TryStreamExt,
    try_join, Stream,
};
use futures::{select, stream::FuturesUnordered};

use log::{debug, error, info, trace, warn};
use serde::Deserialize;
use std::sync::Arc;
use std::{
    marker::PhantomData,
    time::{SystemTime, UNIX_EPOCH},
};

const MAX_HEIGHT_DRIFT: u64 = 5;

// TODO revisit this type, necessary for two sets of Arc<Mutex<>> because each state is
// on separate thread and needs to be mutated independently, but the vec needs to be read
// on the RPC API thread and mutated on this thread.
type WorkerState = Arc<RwLock<Vec<Arc<RwLock<SyncState>>>>>;

type ChainSyncerFuture<T, E> = Pin<Box<dyn Future<Output = Result<T, E>> + Send>>;

/// Struct that defines syncing configuration options
#[derive(Debug, Deserialize, Clone)]
pub struct SyncConfig {
    /// Request window length for tipsets during chain exchange
    pub req_window: i64,
    /// Number of tasks spawned for sync workers
    pub worker_tasks: usize,
}

impl SyncConfig {
    pub fn new(req_window: i64, worker_tasks: usize) -> Self {
        Self {
            req_window,
            worker_tasks,
        }
    }
}
impl Default for SyncConfig {
    // TODO benchmark (1 is temporary value to avoid overlap)
    fn default() -> Self {
        Self {
            req_window: 200,
            worker_tasks: 1,
        }
    }
}

enum NetworkHeadEvaluation {
    Behind {
        network_head: FullTipset,
        local_head: Arc<Tipset>,
    },
    InRange {
        network_head: FullTipset,
    },
    InSync,
}

/// Struct that handles the ChainSync logic. This handles incoming network events such as
/// gossipsub messages, Hello protocol requests, as well as sending and receiving ChainExchange
/// messages to be able to do the initial sync.
pub struct ChainSyncer<DB, TBeacon, V, M> {
    /// State of the ChainSyncer Future implementation
    chain_syncer_state: ChainSyncerState,

    /// Syncing state of chain sync workers.
    worker_state: WorkerState,

    /// Drand randomness beacon
    beacon: Arc<BeaconSchedule<TBeacon>>,

    /// manages retrieving and updates state objects
    state_manager: Arc<StateManager<DB>>,

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

    /// Message pool
    mpool: Arc<MessagePool<M>>,

    /// Tipset channel sender
    tipset_sender: Sender<Arc<Tipset>>,

    /// Tipset channel receiver
    tipset_receiver: Receiver<Arc<Tipset>>,

    /// Syncing configurations
    sync_config: SyncConfig,
}

impl<DB, TBeacon, V, M> ChainSyncer<DB, TBeacon, V, M>
where
    TBeacon: Beacon + Sync + Send + 'static,
    DB: BlockStore + Sync + Send + 'static,
    V: ProofVerifier + Sync + Send + 'static + Unpin,
    M: Provider + Sync + Send + 'static,
{
    pub fn new(
        state_manager: Arc<StateManager<DB>>,
        beacon: Arc<BeaconSchedule<TBeacon>>,
        mpool: Arc<MessagePool<M>>,
        network_send: Sender<NetworkMessage>,
        network_rx: Receiver<NetworkEvent>,
        genesis: Arc<Tipset>,
        tipset_sender: Sender<Arc<Tipset>>,
        tipset_receiver: Receiver<Arc<Tipset>>,
        cfg: SyncConfig,
    ) -> Result<Self, Error> {
        let network = SyncNetworkContext::new(
            network_send,
            Default::default(),
            state_manager.blockstore_cloned(),
        );

        Ok(Self {
            chain_syncer_state: ChainSyncerState::Idle,
            worker_state: Default::default(),
            beacon,
            network,
            genesis,
            state_manager,
            bad_blocks: Arc::new(BadBlockCache::default()),
            net_handler: network_rx,
            verifier: Default::default(),
            mpool,
            tipset_sender,
            tipset_receiver,
            sync_config: cfg,
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

    async fn get_full_tipset(
        network: SyncNetworkContext<DB>,
        chain_store: Arc<ChainStore<DB>>,
        peer_id: PeerId,
        tipset_keys: TipsetKeys,
    ) -> Result<FullTipset, String> {
        let full_tipset = match Self::load_full_tipset(chain_store, &tipset_keys).await {
            Ok(full_tipset) => full_tipset,
            Err(_) => {
                network
                    .chain_exchange_fts(Some(peer_id), &tipset_keys)
                    .await?
            }
        };
        Ok(full_tipset)
    }

    async fn load_full_tipset(
        chain_store: Arc<ChainStore<DB>>,
        tipset_keys: &TipsetKeys,
    ) -> Result<FullTipset, Error> {
        let mut blocks = Vec::new();
        // Retrieve tipset from store based on passed in TipsetKeys
        let ts = chain_store.tipset_from_keys(tipset_keys).await?;
        for header in ts.blocks() {
            // retrieve bls and secp messages from specified BlockHeader
            let (bls_msgs, secp_msgs) = chain::block_messages(chain_store.blockstore(), &header)?;
            // Construct a full block
            blocks.push(Block {
                header: header.clone(),
                bls_messages: bls_msgs,
                secp_messages: secp_msgs,
            });
        }

        // Construct FullTipset
        let fts = FullTipset::new(blocks)?;
        Ok(fts)
    }

    async fn handle_peer_connected_event(
        network: SyncNetworkContext<DB>,
        chain_store: Arc<ChainStore<DB>>,
        peer_id: PeerId,
        genesis_block_cid: Cid,
    ) -> Result<(), String> {
        // Query the heaviest TipSet from the store
        let heaviest = chain_store.heaviest_tipset().await.unwrap();
        // If the peer is new, send them a hello request
        if network.peer_manager().is_peer_new(&peer_id).await {
            let request = HelloRequest {
                heaviest_tip_set: heaviest.cids().to_vec(),
                heaviest_tipset_height: heaviest.epoch(),
                heaviest_tipset_weight: heaviest.weight().clone(),
                genesis_hash: genesis_block_cid,
            };
            let (peer_id, moment_sent, response) =
                match network.hello_request(peer_id, request).await {
                    Ok(response) => response,
                    Err(e) => {
                        error!("{}", e);
                        return Ok(());
                    }
                };
            let dur = SystemTime::now()
                .duration_since(moment_sent)
                .unwrap_or_default();
            match response {
                Some(Ok(_res)) => {
                    network.peer_manager().log_success(peer_id, dur).await;
                }
                Some(Err(why)) => match why {
                    RequestResponseError::ConnectionClosed
                    | RequestResponseError::DialFailure
                    | RequestResponseError::UnsupportedProtocols => {
                        network.peer_manager().mark_peer_bad(peer_id).await;
                    }
                    RequestResponseError::Timeout => {
                        network.peer_manager().log_failure(peer_id, dur).await;
                    }
                },
                None => {
                    network.peer_manager().log_failure(peer_id, dur).await;
                }
            }
        }
        Ok(())
    }

    async fn handle_peer_disconnected_event(
        network: SyncNetworkContext<DB>,
        peer_id: PeerId,
    ) -> Result<(), String> {
        network.peer_manager().remove_peer(&peer_id).await;
        Ok(())
    }

    async fn gossipsub_block_to_full_tipset(
        block: GossipBlock,
        source: PeerId,
        network: SyncNetworkContext<DB>,
    ) -> Result<FullTipset, String> {
        info!(
            "Received block over GossipSub: {} height {} from {}",
            block.header.cid(),
            block.header.epoch(),
            source,
        );

        // Get bls_message in the store or over Bitswap
        let bls_messages: Vec<_> = block
            .bls_messages
            .into_iter()
            .map(|m| network.bitswap_get::<UnsignedMessage>(m))
            .collect();

        // Get secp_messages in the store or over Bitswap
        let secp_messages: Vec<_> = block
            .secpk_messages
            .into_iter()
            .map(|m| network.bitswap_get::<SignedMessage>(m))
            .collect();

        let (bls_messages, secp_messages) =
            match try_join!(try_join_all(bls_messages), try_join_all(secp_messages)) {
                Ok(msgs) => msgs,
                Err(e) => return Err(e.to_string()),
            };

        // From block
        let block = Block {
            header: block.header,
            bls_messages,
            secp_messages,
        };
        Ok(FullTipset::new(vec![block]).unwrap())
    }

    fn handle_pubsub_block(
        network: SyncNetworkContext<DB>,
        block: GossipBlock,
    ) -> Result<(), String> {
        unimplemented!()
    }

    async fn handle_pubsub_message(
        mem_pool: Arc<MessagePool<M>>,
        message: SignedMessage,
    ) -> Result<(), String> {
        if let Err(why) = mem_pool.add(message).await {
            debug!(
                "GossipSub message could not be added to the mem pool: {}",
                why
            );
        }
        Ok(())
    }

    async fn process_gossipsub_event(
        event: NetworkEvent,
        network: SyncNetworkContext<DB>,
        chain_store: Arc<ChainStore<DB>>,
        bad_block_cache: Arc<BadBlockCache>,
        mem_pool: Arc<MessagePool<M>>,
        genesis: Arc<Tipset>,
    ) -> Result<Option<(FullTipset, PeerId)>, String> {
        let (tipset, source) = match event {
            NetworkEvent::HelloRequest { request, source } => {
                let tipset_keys = TipsetKeys::new(request.heaviest_tip_set);
                // Handle hello requests serially.
                // This is OK because we are not yet processing PubSub messages.
                let tipset = Self::get_full_tipset(
                    network.clone(),
                    chain_store.clone(),
                    source,
                    tipset_keys,
                )
                .await
                .unwrap();

                (tipset, source)
            }
            NetworkEvent::PeerConnected(peer_id) => {
                // Spawn and immediately move on to the next event
                async_std::task::spawn(Self::handle_peer_connected_event(
                    network.clone(),
                    chain_store.clone(),
                    peer_id,
                    *genesis.blocks()[0].cid(),
                ));
                return Ok(None);
            }
            NetworkEvent::PeerDisconnected(peer_id) => {
                // Spawn and immediately move on to the next event
                async_std::task::spawn(Self::handle_peer_disconnected_event(
                    network.clone(),
                    peer_id,
                ));
                return Ok(None);
            }
            NetworkEvent::PubsubMessage { source, message } => match message {
                PubsubMessage::Block(b) => {
                    // Assemble full tipset from block
                    // Messages will be persisted when they are pulled from the network
                    let tipset =
                        Self::gossipsub_block_to_full_tipset(b, source, network.clone()).await?;
                    (tipset, source)
                }
                PubsubMessage::Message(m) => {
                    // Spawn and immediately move on to the next event
                    async_std::task::spawn(Self::handle_pubsub_message(mem_pool.clone(), m));
                    return Ok(None);
                }
            },
            // Not supported.
            NetworkEvent::ChainExchangeRequest { .. } | NetworkEvent::BitswapBlock { .. } => {
                return Ok(None);
            }
        };

        // Validate tipset
        if let Err(why) = TipsetValidator(&tipset)
            .validate(
                chain_store.clone(),
                bad_block_cache.clone(),
                genesis.clone(),
            )
            .await
        {
            error!(
                "Validating tipset received through GossipSub failed: {}",
                why
            );
            return Err(why.to_string());
        }

        // Store block messages in the block store
        for block in tipset.blocks() {
            chain::persist_objects(chain_store.db.as_ref(), block.bls_msgs())
                .map_err(|err| err.to_string())?;
            chain::persist_objects(chain_store.db.as_ref(), block.secp_msgs())
                .map_err(|err| err.to_string())?;
        }

        // Update the peer head
        // TODO: Determine if this can be executed asynchronously
        network
            .peer_manager()
            .update_peer_head(source, Arc::new(tipset.clone().into_tipset()))
            .await;

        return Ok(Some((tipset, source)));
    }

    fn evaluate_network_head(&self) -> ChainSyncerFuture<NetworkHeadEvaluation, String> {
        let p2p_messages = self.net_handler.clone();
        let chain_store = self.state_manager.chain_store().clone();
        let network = self.network.clone();
        let genesis = self.genesis.clone();
        let bad_block_cache = self.bad_blocks.clone();
        let mem_pool = self.mpool.clone();

        let evaluator = async move {
            let mut tipsets = vec![];
            let tipset_sample_size = 5usize;
            loop {
                let event = match p2p_messages.recv().await {
                    Ok(event) => event,
                    Err(e) => {
                        // TODO: Return typed error
                        unimplemented!()
                    }
                };

                let (tipset, _) = match Self::process_gossipsub_event(
                    event,
                    network.clone(),
                    chain_store.clone(),
                    bad_block_cache.clone(),
                    mem_pool.clone(),
                    genesis.clone(),
                )
                .await
                {
                    Ok(Some((tipset, source))) => (tipset, source),
                    Ok(None) => continue,
                    Err(why) => {
                        error!("Processing GossipSub event failed: {}", why);
                        continue;
                    }
                };

                // Add to tipset sample
                tipsets.push(tipset);
                if tipsets.len() > tipset_sample_size {
                    break;
                }
            }

            // Find the heaviest tipset in the sample
            // Unwrapping is safe because we ensure the sample size is not 0
            let network_head = tipsets
                .into_iter()
                .max_by_key(|ts| ts.weight().clone())
                .unwrap();
            // Query the heaviest tipset in the store
            let local_head = chain_store.heaviest_tipset().await.unwrap();

            // We are in sync in the local head weight more or
            // as much as the network head
            if local_head.weight() >= network_head.weight() {
                return Ok(NetworkHeadEvaluation::InSync);
            }
            // We are in range if the network epoch is only 1 ahead of
            // the local epoch
            if (network_head.epoch() - local_head.epoch()) == 1 {
                return Ok(NetworkHeadEvaluation::InRange { network_head });
            }
            // Otherwise, we are behind
            return Ok(NetworkHeadEvaluation::Behind {
                network_head,
                local_head,
            });
        };

        Box::pin(evaluator)
    }

    fn bootstrap(
        &self,
        network_head: FullTipset,
        local_head: Arc<Tipset>,
    ) -> ChainSyncerFuture<(), String> {
        let p2p_messages = self.net_handler.clone();
        let chain_store = self.state_manager.chain_store().clone();
        let network = self.network.clone();
        let genesis = self.genesis.clone();
        let beacon = self.beacon.clone();
        let bad_block_cache = self.bad_blocks.clone();
        let mem_pool = self.mpool.clone();
        type BootstrapFuture = Pin<Box<dyn Future<Output = Result<(), String>> + Send>>;

        // Instantiate a TipsetRangeSyncer
        let tipset_range_syncer: BootstrapFuture =
            Box::pin(TipsetRangeSyncer::<DB, TBeacon, V>::new(
                Arc::new(network_head.into_tipset()),
                local_head,
                beacon,
                network.clone(),
                chain_store.clone(),
                bad_block_cache.clone(),
            ));

        // The stream processor _must_ only error if the stream ends
        let stream_processor: BootstrapFuture = Box::pin(async move {
            loop {
                let event = match p2p_messages.recv().await {
                    Ok(event) => event,
                    Err(why) => {
                        // TODO: Return typed error
                        unimplemented!()
                    }
                };

                let (tipset, _) = match Self::process_gossipsub_event(
                    event,
                    network.clone(),
                    chain_store.clone(),
                    bad_block_cache.clone(),
                    mem_pool.clone(),
                    genesis.clone(),
                )
                .await
                {
                    Ok(Some((tipset, source))) => (tipset, source),
                    Ok(None) => continue,
                    Err(why) => {
                        error!("Processing GossipSub event failed: {}", why);
                        continue;
                    }
                };

                // No further processing for the tipset because we are bootstrapping
            }
        });

        let mut tasks = FuturesUnordered::new();
        tasks.push(tipset_range_syncer);
        tasks.push(stream_processor);

        Box::pin(async move {
            match tasks.next().await {
                Some(Ok(_)) => Ok(()),
                Some(Err(e)) => Err(e),
                // This arm is reliably unreachable because the FuturesUnordered
                // has two futures and we only wait for one before returning
                None => unreachable!(),
            }
        })
    }

    fn follow(&self, tipset_opt: Option<FullTipset>) -> ChainSyncerFuture<(), String> {
        let p2p_messages = self.net_handler.clone();
        let chain_store = self.state_manager.chain_store().clone();
        let network = self.network.clone();
        let genesis = self.genesis.clone();
        let beacon = self.beacon.clone();
        let bad_block_cache = self.bad_blocks.clone();
        let mem_pool = self.mpool.clone();
        let tipset_sender = self.tipset_sender.clone();
        let tipset_receiver = self.tipset_receiver.clone();

        type FollowFuture = Pin<Box<dyn Future<Output = Result<(), String>> + Send>>;

        // Instantiate a TipsetProcessor
        let tipset_processor: FollowFuture = Box::pin(TipsetProcessor::<_, _, V>::new(
            Box::pin(tipset_receiver),
            beacon.clone(),
            network.clone(),
            chain_store.clone(),
            bad_block_cache.clone(),
        ));

        // The stream processor _must_ only error if the stream ends
        let stream_processor: FollowFuture = Box::pin(async move {
            // If a tipset has been provided, pass it to the tipset processor
            if let Some(tipset) = tipset_opt {
                if let Err(why) = tipset_sender.send(Arc::new(tipset.into_tipset())).await {
                    error!("Sending tipset to TipsetProcessor failed: {}", why);
                };
            }
            loop {
                let event = match p2p_messages.recv().await {
                    Ok(event) => event,
                    Err(why) => {
                        // TODO: Return typed error
                        unimplemented!()
                    }
                };

                let (tipset, _) = match Self::process_gossipsub_event(
                    event,
                    network.clone(),
                    chain_store.clone(),
                    bad_block_cache.clone(),
                    mem_pool.clone(),
                    genesis.clone(),
                )
                .await
                {
                    Ok(Some((tipset, source))) => (tipset, source),
                    Ok(None) => continue,
                    Err(why) => {
                        error!("Processing GossipSub event failed: {}", why);
                        continue;
                    }
                };

                // Validate that the tipset is heavier that the heaviest
                // tipset in the store
                if let Err(why) = tipset_sender.send(Arc::new(tipset.into_tipset())).await {
                    error!("Sending tipset to TipsetProcessor failed: {}", why);
                };
            }
        });

        let mut tasks = FuturesUnordered::new();
        tasks.push(tipset_processor);
        tasks.push(stream_processor);

        Box::pin(async move {
            match tasks.next().await {
                // Either the TipsetProcessor or the StreamProcessor has returned.
                // Both of these should be long running, so we have to return control
                // back to caller in order to direct the next action.
                Some(Ok(_)) => {
                    // Log & return custom error
                    Ok(())
                }
                Some(Err(e)) => {
                    // Log & return custom error
                    Err(e)
                }
                // This arm is reliably unreachable because the FuturesUnordered
                // has tow futures and we only resolve one before returning
                None => unreachable!(),
            }
        })
    }
}

enum ChainSyncerState {
    Idle,
    Connect(ChainSyncerFuture<NetworkHeadEvaluation, String>),
    Bootstrap(ChainSyncerFuture<(), String>),
    Follow(ChainSyncerFuture<(), String>),
}

impl<DB, TBeacon, V, M> Future for ChainSyncer<DB, TBeacon, V, M>
where
    TBeacon: Beacon + Sync + Send + 'static,
    DB: BlockStore + Sync + Send + 'static,
    V: ProofVerifier + Sync + Send + 'static + Unpin,
    M: Provider + Sync + Send + 'static,
{
    type Output = Result<(), String>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        loop {
            match self.chain_syncer_state {
                ChainSyncerState::Idle => {
                    // Create the connect future and set the state to connect
                    self.chain_syncer_state =
                        ChainSyncerState::Connect(self.evaluate_network_head());
                }
                ChainSyncerState::Connect(ref mut connect) => match connect.as_mut().poll(cx) {
                    Poll::Ready(Ok(evaluation)) => match evaluation {
                        NetworkHeadEvaluation::Behind {
                            network_head,
                            local_head,
                        } => {
                            // TODO: Log
                            self.chain_syncer_state = ChainSyncerState::Bootstrap(
                                self.bootstrap(network_head, local_head),
                            );
                        }
                        NetworkHeadEvaluation::InRange { network_head } => {
                            // TODO: Log
                            self.chain_syncer_state =
                                ChainSyncerState::Follow(self.follow(Some(network_head)));
                        }
                        NetworkHeadEvaluation::InSync => {
                            // TODO: Log
                            self.chain_syncer_state = ChainSyncerState::Follow(self.follow(None));
                        }
                    },
                    Poll::Ready(Err(_why)) => {
                        // TODO: Determine error handling strategy here
                    }
                    Poll::Pending => return Poll::Pending,
                },
                ChainSyncerState::Bootstrap(ref mut bootstrap) => match bootstrap.as_mut().poll(cx)
                {
                    Poll::Ready(Ok(_)) => {
                        // TODO: Log
                        self.chain_syncer_state = ChainSyncerState::Idle;
                    }
                    Poll::Ready(Err(_why)) => {
                        // TODO: Determine error handling strategy here
                        // TODO: Log
                        self.chain_syncer_state = ChainSyncerState::Idle;
                    }
                    Poll::Pending => return Poll::Pending,
                },
                ChainSyncerState::Follow(ref mut follow) => match follow.as_mut().poll(cx) {
                    Poll::Ready(Ok(_)) => {
                        // TODO: Figure out what to do when the follow future completes
                        // TODO: Log
                        self.chain_syncer_state = ChainSyncerState::Idle;
                    }
                    Poll::Ready(Err(_why)) => {
                        // TODO: Determine error handling strategy here
                        // TODO: Log
                        self.chain_syncer_state = ChainSyncerState::Idle;
                    }
                    Poll::Pending => return Poll::Pending,
                },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_std::channel::{bounded, Sender};
    use async_std::task;
    use beacon::{BeaconPoint, MockBeacon};
    use db::MemoryDB;
    use fil_types::verifier::MockVerifier;
    use forest_libp2p::NetworkEvent;
    use message_pool::{test_provider::TestApi, MessagePool};
    use state_manager::StateManager;
    use std::convert::TryFrom;
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
        let (tx, _rx) = bounded(10);
        let mpool = task::block_on(MessagePool::new(
            test_provider,
            "test".to_string(),
            tx,
            Default::default(),
        ))
        .unwrap();
        let mpool = Arc::new(mpool);
        let (local_sender, test_receiver) = bounded(20);
        let (event_sender, event_receiver) = bounded(20);

        let gen = construct_dummy_header();
        chain_store.set_genesis(&gen).unwrap();

        let beacon = Arc::new(BeaconSchedule(vec![BeaconPoint {
            height: 0,
            beacon: Arc::new(MockBeacon::new(Duration::from_secs(1))),
        }]));

        let genesis_ts = Arc::new(Tipset::new(vec![gen]).unwrap());
        (
            ChainSyncer::new(
                Arc::new(StateManager::new(chain_store)),
                beacon,
                mpool,
                local_sender,
                event_receiver,
                genesis_ts,
                SyncConfig::default(),
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
            Cid::try_from("bafy2bzaceasssikoiintnok7f3sgnekfifarzobyr3r4f25sgxmn23q4c35ic")
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
