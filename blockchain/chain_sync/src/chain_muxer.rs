// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::bad_block_cache::BadBlockCache;
use crate::metrics;
use crate::network_context::SyncNetworkContext;
use crate::sync_state::SyncState;
use crate::tipset_syncer::{
    TipsetProcessor, TipsetProcessorError, TipsetRangeSyncer, TipsetRangeSyncerError,
};
use crate::validation::{TipsetValidationError, TipsetValidator};

use beacon::{Beacon, BeaconSchedule};
use blocks::{Block, Error as ForestBlockError, FullTipset, GossipBlock, Tipset, TipsetKeys};
use chain::{ChainStore, Error as ChainStoreError};
use cid::Cid;
use fil_types::verifier::ProofVerifier;
use forest_libp2p::{
    hello::HelloRequest, rpc::RequestResponseError, NetworkEvent, NetworkMessage, PubsubMessage,
};
use ipld_blockstore::BlockStore;
use libp2p::core::PeerId;
use message::{SignedMessage, UnsignedMessage};
use message_pool::{MessagePool, Provider};
use state_manager::StateManager;

use async_std::channel::{Receiver, Sender};
use async_std::pin::Pin;
use async_std::stream::StreamExt;
use async_std::sync::RwLock;
use async_std::task::{Context, Poll};
use futures::stream::FuturesUnordered;
use futures::{future::try_join_all, future::Future, try_join};
use log::{debug, error, info, trace, warn};
use serde::Deserialize;
use thiserror::Error;

use std::sync::Arc;
use std::{marker::PhantomData, time::SystemTime};

pub(crate) type WorkerState = Arc<RwLock<SyncState>>;

type ChainMuxerFuture<T, E> = Pin<Box<dyn Future<Output = Result<T, E>> + Send>>;

#[derive(Debug, Error)]
pub enum ChainMuxerError {
    #[error("Tipset processor error: {0}")]
    TipsetProcessor(#[from] TipsetProcessorError),
    #[error("Tipset range syncer error: {0}")]
    TipsetRangeSyncer(#[from] TipsetRangeSyncerError),
    #[error("Tipset validation error: {0}")]
    TipsetValidator(#[from] TipsetValidationError),
    #[error("Sending tipset on channel failed: {0}")]
    TipsetChannelSend(String),
    #[error("Receiving p2p network event failed: {0}")]
    P2PEventStreamReceive(String),
    #[error("Chain store error: {0}")]
    ChainStore(#[from] ChainStoreError),
    #[error("Chain exchange: {0}")]
    ChainExchange(String),
    #[error("Bitswap: {0}")]
    Bitswap(String),
    #[error("Block error: {0}")]
    Block(#[from] ForestBlockError),
    #[error("Following network unexpectedly failed: {0}")]
    NetworkFollowingFailure(String),
}

/// Struct that defines syncing configuration options
#[derive(Debug, Deserialize, Clone)]
pub struct SyncConfig {
    /// Request window length for tipsets during chain exchange
    pub req_window: i64,
    /// Sample size of tipsets to acquire before determining what the network head is
    pub tipset_sample_size: usize,
}

impl SyncConfig {
    pub fn new(req_window: i64, tipset_sample_size: usize) -> Self {
        Self {
            req_window,
            tipset_sample_size,
        }
    }
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            req_window: 200,
            tipset_sample_size: 5,
        }
    }
}

/// Represents the result of evaluating the network head tipset against the
/// local head tipset
enum NetworkHeadEvaluation {
    /// Local head is behind the network and needs move into the BOOTSTRAP state
    Behind {
        network_head: FullTipset,
        local_head: Arc<Tipset>,
    },
    /// Local head is the direct ancestor of the network head. The node should
    /// move into the FOLLOW state and immediately sync the network head
    InRange { network_head: FullTipset },
    /// Local head is at the same height as the network head. The node should
    /// move into the FOLLOW state and wait for new tipsets
    InSync,
}

/// Repesents whether received messages should be added to message pool
enum PubsubMessageProcessingStrategy {
    /// Messages should be added to the message pool
    Process,
    /// Message _should not_ be added to the message pool
    DoNotProcess,
}

/// The ChainMuxer handles events from the p2p network and orchestrates the chain synchronization.
pub struct ChainMuxer<DB, TBeacon, V, M> {
    /// State of the ChainSyncer Future implementation
    state: ChainMuxerState,

    /// Syncing state of chain sync workers.
    worker_state: WorkerState,

    /// Drand randomness beacon
    beacon: Arc<BeaconSchedule<TBeacon>>,

    /// manages retrieving and updates state objects
    state_manager: Arc<StateManager<DB>>,

    /// Context to be able to send requests to p2p network
    network: SyncNetworkContext<DB>,

    /// Genesis tipset
    genesis: Arc<Tipset>,

    /// Bad blocks cache, updates based on invalid state transitions.
    /// Will mark any invalid blocks and all childen as bad in this bounded cache
    bad_blocks: Arc<BadBlockCache>,

    /// Incoming network events to be handled by syncer
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

impl<DB, TBeacon, V, M> ChainMuxer<DB, TBeacon, V, M>
where
    TBeacon: Beacon + Sync + Send + 'static,
    DB: BlockStore + Sync + Send + 'static,
    V: ProofVerifier + Sync + Send + 'static + Unpin,
    M: Provider + Sync + Send + 'static,
{
    #[allow(clippy::too_many_arguments)]
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
    ) -> Result<Self, ChainMuxerError> {
        let network = SyncNetworkContext::new(
            network_send,
            Default::default(),
            state_manager.blockstore_cloned(),
        );

        Ok(Self {
            state: ChainMuxerState::Idle,
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
    ) -> Result<FullTipset, ChainMuxerError> {
        // Attempt to load from the store
        if let Ok(full_tipset) = Self::load_full_tipset(chain_store, tipset_keys.clone()).await {
            return Ok(full_tipset);
        }
        // Load from the network
        network
            .chain_exchange_fts(Some(peer_id), &tipset_keys.clone())
            .await
            .map_err(ChainMuxerError::ChainExchange)
    }

    async fn load_full_tipset(
        chain_store: Arc<ChainStore<DB>>,
        tipset_keys: TipsetKeys,
    ) -> Result<FullTipset, ChainMuxerError> {
        let mut blocks = Vec::new();
        // Retrieve tipset from store based on passed in TipsetKeys
        let ts = chain_store.tipset_from_keys(&tipset_keys).await?;
        for header in ts.blocks() {
            // Retrieve bls and secp messages from specified BlockHeader
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
    ) {
        // Query the heaviest TipSet from the store
        let heaviest = chain_store.heaviest_tipset().await.unwrap();
        if network.peer_manager().is_peer_new(&peer_id).await {
            // Since the peer is new, send them a hello request
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
                        return;
                    }
                };
            let dur = SystemTime::now()
                .duration_since(moment_sent)
                .unwrap_or_default();

            // Update the peer metadata based on the response
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
    }

    async fn handle_peer_disconnected_event(network: SyncNetworkContext<DB>, peer_id: PeerId) {
        network.peer_manager().remove_peer(&peer_id).await;
    }

    async fn gossipsub_block_to_full_tipset(
        block: GossipBlock,
        source: PeerId,
        network: SyncNetworkContext<DB>,
    ) -> Result<FullTipset, ChainMuxerError> {
        debug!(
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
                Err(e) => return Err(ChainMuxerError::Bitswap(e)),
            };

        let block = Block {
            header: block.header,
            bls_messages,
            secp_messages,
        };
        Ok(FullTipset::new(vec![block]).unwrap())
    }

    async fn handle_pubsub_message(mem_pool: Arc<MessagePool<M>>, message: SignedMessage) {
        if let Err(why) = mem_pool.add(message).await {
            debug!(
                "GossipSub message could not be added to the mem pool: {}",
                why
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn process_gossipsub_event(
        event: NetworkEvent,
        network: SyncNetworkContext<DB>,
        chain_store: Arc<ChainStore<DB>>,
        bad_block_cache: Arc<BadBlockCache>,
        mem_pool: Arc<MessagePool<M>>,
        genesis: Arc<Tipset>,
        message_processing_strategy: PubsubMessageProcessingStrategy,
    ) -> Result<Option<(FullTipset, PeerId)>, ChainMuxerError> {
        let (tipset, source) = match event {
            NetworkEvent::HelloRequest { request, source } => {
                metrics::LIBP2P_MESSAGE_TOTAL
                    .with_label_values(&[metrics::values::HELLO_REQUEST])
                    .inc();
                let tipset_keys = TipsetKeys::new(request.heaviest_tip_set);
                let tipset = match Self::get_full_tipset(
                    network.clone(),
                    chain_store.clone(),
                    source,
                    tipset_keys,
                )
                .await
                {
                    Ok(tipset) => tipset,
                    Err(why) => {
                        error!("Querying full tipset failed: {}", why);
                        return Err(why);
                    }
                };
                (tipset, source)
            }
            NetworkEvent::PeerConnected(peer_id) => {
                metrics::LIBP2P_MESSAGE_TOTAL
                    .with_label_values(&[metrics::values::PEER_CONNECTED])
                    .inc();
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
                metrics::LIBP2P_MESSAGE_TOTAL
                    .with_label_values(&[metrics::values::PEER_DISCONNECTED])
                    .inc();
                // Spawn and immediately move on to the next event
                async_std::task::spawn(Self::handle_peer_disconnected_event(
                    network.clone(),
                    peer_id,
                ));
                return Ok(None);
            }
            NetworkEvent::PubsubMessage { source, message } => match message {
                PubsubMessage::Block(b) => {
                    metrics::LIBP2P_MESSAGE_TOTAL
                        .with_label_values(&[metrics::values::PUBSUB_BLOCK])
                        .inc();
                    // Assemble full tipset from block
                    let tipset =
                        Self::gossipsub_block_to_full_tipset(b, source, network.clone()).await?;
                    (tipset, source)
                }
                PubsubMessage::Message(m) => {
                    metrics::LIBP2P_MESSAGE_TOTAL
                        .with_label_values(&[metrics::values::PUBSUB_MESSAGE])
                        .inc();
                    if let PubsubMessageProcessingStrategy::Process = message_processing_strategy {
                        // Spawn and immediately move on to the next event
                        async_std::task::spawn(Self::handle_pubsub_message(mem_pool.clone(), m));
                    }
                    return Ok(None);
                }
            },
            NetworkEvent::ChainExchangeRequest { .. } => {
                metrics::LIBP2P_MESSAGE_TOTAL
                    .with_label_values(&[metrics::values::CHAIN_EXCHANGE_REQUEST])
                    .inc();
                // Not supported.
                return Ok(None);
            }
            NetworkEvent::BitswapBlock { .. } => {
                metrics::LIBP2P_MESSAGE_TOTAL
                    .with_label_values(&[metrics::values::BITSWAP_BLOCK])
                    .inc();
                // Not supported.
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
            metrics::INVALID_TIPSET_TOTAL.inc();
            warn!(
                "Validating tipset received through GossipSub failed: {}",
                why
            );
            return Err(why.into());
        }

        // Store block messages in the block store
        for block in tipset.blocks() {
            chain::persist_objects(chain_store.db.as_ref(), &[block.header()])?;
            chain::persist_objects(chain_store.db.as_ref(), block.bls_msgs())?;
            chain::persist_objects(chain_store.db.as_ref(), block.secp_msgs())?;
        }

        // Update the peer head
        // TODO: Determine if this can be executed concurrently
        network
            .peer_manager()
            .update_peer_head(source, Arc::new(tipset.clone().into_tipset()))
            .await;

        Ok(Some((tipset, source)))
    }

    fn evaluate_network_head(&self) -> ChainMuxerFuture<NetworkHeadEvaluation, ChainMuxerError> {
        let p2p_messages = self.net_handler.clone();
        let chain_store = self.state_manager.chain_store().clone();
        let network = self.network.clone();
        let genesis = self.genesis.clone();
        let bad_block_cache = self.bad_blocks.clone();
        let mem_pool = self.mpool.clone();
        let tipset_sample_size = self.sync_config.tipset_sample_size;

        let evaluator = async move {
            let mut tipsets = vec![];
            loop {
                let event = match p2p_messages.recv().await {
                    Ok(event) => event,
                    Err(why) => {
                        error!("Receiving event from p2p event stream failed: {}", why);
                        return Err(ChainMuxerError::P2PEventStreamReceive(why.to_string()));
                    }
                };

                let (tipset, _) = match Self::process_gossipsub_event(
                    event,
                    network.clone(),
                    chain_store.clone(),
                    bad_block_cache.clone(),
                    mem_pool.clone(),
                    genesis.clone(),
                    PubsubMessageProcessingStrategy::Process,
                )
                .await
                {
                    Ok(Some((tipset, source))) => (tipset, source),
                    Ok(None) => continue,
                    Err(why) => {
                        debug!("Processing GossipSub event failed: {:?}", why);
                        continue;
                    }
                };

                // Add to tipset sample
                tipsets.push(tipset);
                if tipsets.len() >= tipset_sample_size {
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
            // Unwrapping is fine because the store always has at least one tipset
            let local_head = chain_store.heaviest_tipset().await.unwrap();

            // We are in sync if the local head weight is heavier or
            // as heavy as the network head
            if local_head.weight() >= network_head.weight() {
                return Ok(NetworkHeadEvaluation::InSync);
            }
            // We are in range if the network epoch is only 1 ahead of the local epoch
            if (network_head.epoch() - local_head.epoch()) == 1 {
                return Ok(NetworkHeadEvaluation::InRange { network_head });
            }
            // Local node is behind the network and we need to do an initial sync
            Ok(NetworkHeadEvaluation::Behind {
                network_head,
                local_head,
            })
        };

        Box::pin(evaluator)
    }

    fn bootstrap(
        &self,
        network_head: FullTipset,
        local_head: Arc<Tipset>,
    ) -> ChainMuxerFuture<(), ChainMuxerError> {
        // Instantiate a TipsetRangeSyncer
        let trs_state_manager = self.state_manager.clone();
        let trs_bad_block_cache = self.bad_blocks.clone();
        let trs_chain_store = self.state_manager.chain_store().clone();
        let trs_network = self.network.clone();
        let trs_beacon = self.beacon.clone();
        let trs_tracker = self.worker_state.clone();
        let trs_genesis = self.genesis.clone();
        let tipset_range_syncer: ChainMuxerFuture<(), ChainMuxerError> = Box::pin(async move {
            let network_head_epoch = network_head.epoch();
            let tipset_range_syncer = match TipsetRangeSyncer::<DB, TBeacon, V>::new(
                trs_tracker,
                Arc::new(network_head.into_tipset()),
                local_head,
                trs_state_manager,
                trs_beacon,
                trs_network,
                trs_chain_store,
                trs_bad_block_cache,
                trs_genesis,
            ) {
                Ok(tipset_range_syncer) => tipset_range_syncer,
                Err(why) => {
                    metrics::TIPSET_RANGE_SYNC_FAILURE_TOTAL.inc();
                    return Err(ChainMuxerError::TipsetRangeSyncer(why));
                }
            };

            tipset_range_syncer
                .await
                .map_err(ChainMuxerError::TipsetRangeSyncer)?;

            metrics::HEAD_EPOCH.set(network_head_epoch as u64);

            Ok(())
        });

        // The stream processor _must_ only error if the stream ends
        let p2p_messages = self.net_handler.clone();
        let chain_store = self.state_manager.chain_store().clone();
        let network = self.network.clone();
        let genesis = self.genesis.clone();
        let bad_block_cache = self.bad_blocks.clone();
        let mem_pool = self.mpool.clone();
        let stream_processor: ChainMuxerFuture<(), ChainMuxerError> = Box::pin(async move {
            loop {
                let event = match p2p_messages.recv().await {
                    Ok(event) => event,
                    Err(why) => {
                        error!("Receiving event from p2p event stream failed: {}", why);
                        return Err(ChainMuxerError::P2PEventStreamReceive(why.to_string()));
                    }
                };

                let (_tipset, _) = match Self::process_gossipsub_event(
                    event,
                    network.clone(),
                    chain_store.clone(),
                    bad_block_cache.clone(),
                    mem_pool.clone(),
                    genesis.clone(),
                    PubsubMessageProcessingStrategy::DoNotProcess,
                )
                .await
                {
                    Ok(Some((tipset, source))) => (tipset, source),
                    Ok(None) => continue,
                    Err(why) => {
                        debug!("Processing GossipSub event failed: {:?}", why);
                        continue;
                    }
                };

                // Drop tipsets while we are bootstrapping
            }
        });

        let mut tasks = FuturesUnordered::new();
        tasks.push(tipset_range_syncer);
        tasks.push(stream_processor);

        Box::pin(async move {
            // The stream processor will not return unless the p2p event stream is closed. In this case it will return with an error.
            // Only wait for one task to complete before returning to the caller
            match tasks.next().await {
                Some(Ok(_)) => Ok(()),
                Some(Err(e)) => Err(e),
                // This arm is reliably unreachable because the FuturesUnordered
                // has two futures and we only wait for one before returning
                None => unreachable!(),
            }
        })
    }

    fn follow(&self, tipset_opt: Option<FullTipset>) -> ChainMuxerFuture<(), ChainMuxerError> {
        // Instantiate a TipsetProcessor
        let tp_state_manager = self.state_manager.clone();
        let tp_beacon = self.beacon.clone();
        let tp_network = self.network.clone();
        let tp_chain_store = self.state_manager.chain_store().clone();
        let tp_bad_block_cache = self.bad_blocks.clone();
        let tp_tipset_receiver = self.tipset_receiver.clone();
        let tp_tracker = self.worker_state.clone();
        let tp_genesis = self.genesis.clone();
        enum UnexpectedReturnKind {
            TipsetProcessor,
        }
        let tipset_processor: ChainMuxerFuture<UnexpectedReturnKind, ChainMuxerError> =
            Box::pin(async move {
                TipsetProcessor::<_, _, V>::new(
                    tp_tracker,
                    Box::pin(tp_tipset_receiver),
                    tp_state_manager,
                    tp_beacon,
                    tp_network,
                    tp_chain_store,
                    tp_bad_block_cache,
                    tp_genesis,
                )
                .await
                .map_err(ChainMuxerError::TipsetProcessor)?;

                Ok(UnexpectedReturnKind::TipsetProcessor)
            });

        // The stream processor _must_ only error if the p2p event stream ends or if the
        // tipset channel is unexpectedly closed
        let p2p_messages = self.net_handler.clone();
        let chain_store = self.state_manager.chain_store().clone();
        let network = self.network.clone();
        let genesis = self.genesis.clone();
        let bad_block_cache = self.bad_blocks.clone();
        let mem_pool = self.mpool.clone();
        let tipset_sender = self.tipset_sender.clone();
        let stream_processor: ChainMuxerFuture<UnexpectedReturnKind, ChainMuxerError> = Box::pin(
            async move {
                // If a tipset has been provided, pass it to the tipset processor
                if let Some(tipset) = tipset_opt {
                    if let Err(why) = tipset_sender.send(Arc::new(tipset.into_tipset())).await {
                        error!("Sending tipset to TipsetProcessor failed: {}", why);
                        return Err(ChainMuxerError::TipsetChannelSend(why.to_string()));
                    };
                }
                loop {
                    let event = match p2p_messages.recv().await {
                        Ok(event) => event,
                        Err(why) => {
                            error!("Receiving event from p2p event stream failed: {}", why);
                            return Err(ChainMuxerError::P2PEventStreamReceive(why.to_string()));
                        }
                    };

                    let (tipset, _) = match Self::process_gossipsub_event(
                        event,
                        network.clone(),
                        chain_store.clone(),
                        bad_block_cache.clone(),
                        mem_pool.clone(),
                        genesis.clone(),
                        PubsubMessageProcessingStrategy::Process,
                    )
                    .await
                    {
                        Ok(Some((tipset, source))) => (tipset, source),
                        Ok(None) => continue,
                        Err(why) => {
                            debug!("Processing GossipSub event failed: {:?}", why);
                            continue;
                        }
                    };

                    // Validate that the tipset is heavier that the heaviest
                    // tipset in the store
                    if !chain_store
                        .heaviest_tipset()
                        .await
                        .map(|heaviest| tipset.weight() >= heaviest.weight())
                        .unwrap_or(true)
                    {
                        // Only send heavier Tipsets to the TipsetProcessor
                        trace!("Dropping tipset [Key = {:?}] that is not heavier than the heaviest tipset in the store", tipset.key());
                        continue;
                    }

                    if let Err(why) = tipset_sender.send(Arc::new(tipset.into_tipset())).await {
                        error!("Sending tipset to TipsetProcessor failed: {}", why);
                        return Err(ChainMuxerError::TipsetChannelSend(why.to_string()));
                    };
                }
            },
        );

        let mut tasks = FuturesUnordered::new();
        tasks.push(tipset_processor);
        tasks.push(stream_processor);

        Box::pin(async move {
            // Only wait for one of the tasks to complete before returning to the caller
            match tasks.next().await {
                // Either the TipsetProcessor or the StreamProcessor has returned.
                // Both of these should be long running, so we have to return control
                // back to caller in order to direct the next action.
                Some(Ok(kind)) => {
                    // Log the expected return
                    match kind {
                        UnexpectedReturnKind::TipsetProcessor => {
                            Err(ChainMuxerError::NetworkFollowingFailure(String::from(
                                "Tipset processor unexpectedly returned",
                            )))
                        }
                    }
                }
                Some(Err(e)) => {
                    error!("Following the network failed unexpectedly: {}", e);
                    Err(e)
                }
                // This arm is reliably unreachable because the FuturesUnordered
                // has two futures and we only resolve one before returning
                None => unreachable!(),
            }
        })
    }
}

enum ChainMuxerState {
    Idle,
    Connect(ChainMuxerFuture<NetworkHeadEvaluation, ChainMuxerError>),
    Bootstrap(ChainMuxerFuture<(), ChainMuxerError>),
    Follow(ChainMuxerFuture<(), ChainMuxerError>),
}

impl<DB, TBeacon, V, M> Future for ChainMuxer<DB, TBeacon, V, M>
where
    TBeacon: Beacon + Sync + Send + 'static,
    DB: BlockStore + Sync + Send + 'static,
    V: ProofVerifier + Sync + Send + 'static + Unpin,
    M: Provider + Sync + Send + 'static,
{
    type Output = ChainMuxerError;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        loop {
            match self.state {
                ChainMuxerState::Idle => {
                    // Create the connect future and set the state to connect
                    self.state = ChainMuxerState::Connect(self.evaluate_network_head());
                }
                ChainMuxerState::Connect(ref mut connect) => match connect.as_mut().poll(cx) {
                    Poll::Ready(Ok(evaluation)) => match evaluation {
                        NetworkHeadEvaluation::Behind {
                            network_head,
                            local_head,
                        } => {
                            info!("Local node is behind the network, starting BOOTSTRAP from LOCAL_HEAD = {} -> NETWORK_HEAD = {}", local_head.epoch(), network_head.epoch());
                            self.state = ChainMuxerState::Bootstrap(
                                self.bootstrap(network_head, local_head),
                            );
                        }
                        NetworkHeadEvaluation::InRange { network_head } => {
                            info!("Local node is within range of the NETWORK_HEAD = {}, starting FOLLOW", network_head.epoch());
                            self.state = ChainMuxerState::Follow(self.follow(Some(network_head)));
                        }
                        NetworkHeadEvaluation::InSync => {
                            info!("Local node is in sync with the network");
                            self.state = ChainMuxerState::Follow(self.follow(None));
                        }
                    },
                    Poll::Ready(Err(why)) => {
                        // TODO: Should we exponentially backoff before retrying?
                        error!(
                            "Evaluating the network head failed, retrying. Error = {:?}",
                            why
                        );
                        self.state = ChainMuxerState::Idle;
                    }
                    Poll::Pending => return Poll::Pending,
                },
                ChainMuxerState::Bootstrap(ref mut bootstrap) => {
                    match bootstrap.as_mut().poll(cx) {
                        Poll::Ready(Ok(_)) => {
                            info!("Bootstrap successfully completed, now evaluating the network head to ensure the node is in sync");
                            self.state = ChainMuxerState::Idle;
                        }
                        Poll::Ready(Err(why)) => {
                            // TODO: Should we exponentially back off before retrying?
                            error!("Bootstrapping failed, re-evaluating the network head to retry the bootstrap. Error = {:?}", why);
                            self.state = ChainMuxerState::Idle;
                        }
                        Poll::Pending => return Poll::Pending,
                    }
                }
                ChainMuxerState::Follow(ref mut follow) => match follow.as_mut().poll(cx) {
                    Poll::Ready(Ok(_)) => {
                        error!("Following the network unexpectedly ended without an error; restarting the sync process.");
                        self.state = ChainMuxerState::Idle;
                    }
                    Poll::Ready(Err(why)) => {
                        error!("Following the network failed, restarted. Error = {:?}", why);
                        self.state = ChainMuxerState::Idle;
                    }
                    Poll::Pending => return Poll::Pending,
                },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::convert::TryFrom;

    use crate::validation::TipsetValidator;
    use cid::Cid;
    use db::MemoryDB;
    use message::{SignedMessage, UnsignedMessage};
    use test_utils::construct_messages;

    #[test]
    fn compute_msg_meta_given_msgs_test() {
        let blockstore = MemoryDB::default();

        let (bls, secp) = construct_messages();

        let expected_root =
            Cid::try_from("bafy2bzaceasssikoiintnok7f3sgnekfifarzobyr3r4f25sgxmn23q4c35ic")
                .unwrap();

        let root = TipsetValidator::compute_msg_root(&blockstore, &[bls], &[secp])
            .expect("Computing message root should succeed");
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
            TipsetValidator::compute_msg_root(&blockstore, &usm, &sm)
                .expect("Computing message root should succeed")
                .to_string(),
            "bafy2bzacecmda75ovposbdateg7eyhwij65zklgyijgcjwynlklmqazpwlhba"
        );
    }
}
