// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use std::time::SystemTime;
use std::{ops::Deref as _, sync::Arc};

use crate::libp2p::hello::HelloRequest;
use crate::message_pool::MessagePool;
use crate::message_pool::MpoolRpcProvider;
use crate::shim::clock::ChainEpoch;
use crate::state_manager::StateManager;
use ahash::{HashMap, HashSet};
use chrono::Utc;
use cid::Cid;
use fvm_ipld_blockstore::Blockstore;
use itertools::Itertools;
use libp2p::PeerId;
use parking_lot::Mutex;
use tokio::{sync::Notify, task::JoinSet};
use tracing::{debug, info, trace, warn};

use crate::chain_sync::tipset_syncer::validate_tipset;
use crate::chain_sync::tipset_syncer::InvalidBlockStrategy;
use crate::chain_sync::SyncState;
use crate::{
    blocks::{Block, FullTipset, Tipset, TipsetKey},
    chain::ChainStore,
    chain_sync::{bad_block_cache::BadBlockCache, metrics, TipsetValidator},
    libp2p::{NetworkEvent, PubsubMessage},
    networks::ChainConfig,
    shim::clock::SECONDS_IN_DAY,
};
use parking_lot::RwLock;

use super::network_context::SyncNetworkContext;
use super::SyncStage;

pub struct ChainFollower<DB> {
    /// Syncing state of chain sync workers.
    pub sync_states: Arc<RwLock<Vec<SyncState>>>,

    /// manages retrieving and updates state objects
    state_manager: Arc<StateManager<DB>>,

    /// Context to be able to send requests to P2P network
    pub network: SyncNetworkContext<DB>,

    /// Genesis tipset
    _genesis: Arc<Tipset>,

    /// Bad blocks cache, updates based on invalid state transitions.
    /// Will mark any invalid blocks and all children as bad in this bounded
    /// cache
    pub bad_blocks: Arc<BadBlockCache>,

    /// Incoming network events to be handled by synchronizer
    net_handler: flume::Receiver<NetworkEvent>,

    /// Tipset channel sender
    pub tipset_sender: flume::Sender<Arc<FullTipset>>,

    /// Tipset channel receiver
    tipset_receiver: flume::Receiver<Arc<FullTipset>>,

    /// When `stateless_mode` is true, forest connects to the P2P network but does not sync to HEAD.
    _stateless_mode: bool,

    /// Message pool
    mem_pool: Arc<MessagePool<MpoolRpcProvider<DB>>>,
}

impl<DB: Blockstore + Sync + Send + 'static> ChainFollower<DB> {
    pub fn new(
        state_manager: Arc<StateManager<DB>>,
        network: SyncNetworkContext<DB>,
        genesis: Arc<Tipset>,
        net_handler: flume::Receiver<NetworkEvent>,
        stateless_mode: bool,
        mem_pool: Arc<MessagePool<MpoolRpcProvider<DB>>>,
    ) -> Self {
        let heaviest = state_manager.chain_store().heaviest_tipset();
        let mut main_sync_state = SyncState::default();
        main_sync_state.init(heaviest.clone(), heaviest.clone());
        main_sync_state.set_epoch(heaviest.epoch());
        main_sync_state.set_stage(SyncStage::Messages);
        let (tipset_sender, tipset_receiver) = flume::bounded(20);
        Self {
            sync_states: Arc::new(RwLock::new(vec![main_sync_state])),
            state_manager,
            network,
            _genesis: genesis,
            bad_blocks: Default::default(),
            net_handler,
            tipset_sender,
            tipset_receiver,
            _stateless_mode: stateless_mode,
            mem_pool,
        }
    }

    pub async fn run(self) -> anyhow::Result<()> {
        chain_follower(
            self.state_manager,
            self.bad_blocks,
            self.net_handler,
            self.tipset_receiver,
            self.network,
            self.mem_pool,
            self.sync_states,
            self._genesis,
        )
        .await
    }
}

#[allow(clippy::too_many_arguments)]
// We receive new full tipsets from the p2p swarm, and from miners that use Forest as their frontend.
pub async fn chain_follower<DB: Blockstore + Sync + Send + 'static>(
    state_manager: Arc<StateManager<DB>>,
    bad_block_cache: Arc<BadBlockCache>,
    network_rx: flume::Receiver<NetworkEvent>,
    tipset_receiver: flume::Receiver<Arc<FullTipset>>,
    network: SyncNetworkContext<DB>,
    mem_pool: Arc<MessagePool<MpoolRpcProvider<DB>>>,
    sync_states: Arc<RwLock<Vec<SyncState>>>,
    genesis: Arc<Tipset>,
) -> anyhow::Result<()> {
    let state_changed = Arc::new(Notify::new());
    let state_machine = Arc::new(Mutex::new(SyncStateMachine::new(
        state_manager.chain_store().clone(),
        state_manager.chain_config().clone(),
        bad_block_cache.clone(),
    )));
    let tasks: Arc<Mutex<HashSet<SyncTask>>> = Arc::new(Mutex::new(HashSet::default()));

    let mut set = JoinSet::new();

    set.spawn({
        let state_manager = state_manager.clone();
        let state_changed = state_changed.clone();
        let state_machine = state_machine.clone();
        let network = network.clone();
        async move {
            while let Ok(event) = network_rx.recv_async().await {
                inc_gossipsub_event_metrics(&event);

                upd_peer_information(
                    &event,
                    network.clone(),
                    state_manager.chain_store().clone(),
                    &genesis,
                );

                let Ok(tipset) = (match event {
                    NetworkEvent::HelloResponseOutbound { request, source } => {
                        let tipset_keys = TipsetKey::from(request.heaviest_tip_set.clone());
                        get_full_tipset(
                            network.clone(),
                            state_manager.chain_store().clone(),
                            Some(source),
                            tipset_keys,
                        )
                        .await
                        .inspect_err(|e| debug!("Querying full tipset failed: {}", e))
                    }
                    NetworkEvent::PubsubMessage { message } => match message {
                        PubsubMessage::Block(b) => {
                            let key = TipsetKey::from(nunny::vec![*b.header.cid()]);
                            get_full_tipset(
                                network.clone(),
                                state_manager.chain_store().clone(),
                                None,
                                key,
                            )
                            .await
                        }
                        PubsubMessage::Message(m) => {
                            if let Err(why) = mem_pool.add(m) {
                                debug!(
                                    "GossipSub message could not be added to the mem pool: {}",
                                    why
                                );
                            }
                            continue;
                        }
                    },
                    _ => continue,
                }) else {
                    continue;
                };
                {
                    state_machine
                        .lock()
                        .update(SyncEvent::NewFullTipsets(vec![Arc::new(tipset)]));
                    state_changed.notify_one();
                }
            }
        }
    });

    // spawn a task to tipsets to the state machine. These tipsets are received
    // from the p2p swarm and from directly-connected miners.
    set.spawn({
        let state_changed = state_changed.clone();
        let state_machine = state_machine.clone();

        async move {
            while let Ok(tipset) = tipset_receiver.recv_async().await {
                {
                    state_machine
                        .lock()
                        .update(SyncEvent::NewFullTipsets(vec![tipset]));
                    state_changed.notify_one();
                }
            }
            // tipset_receiver is closed, shutdown gracefully
        }
    });

    set.spawn({
        let state_manager = state_manager.clone();
        let state_machine = state_machine.clone();
        let state_changed = state_changed.clone();
        let bad_block_cache = bad_block_cache.clone();
        let tasks = tasks.clone();
        async move {
            loop {
                state_changed.notified().await;

                let mut tasks_set = tasks.lock();
                let (task_vec, states) = state_machine.lock().tasks();

                {
                    let heaviest = state_manager.chain_store().heaviest_tipset();
                    let mut sync_states_guard = sync_states.write();
                    // info!("Number of sync states: {}", sync_states_guard.len());
                    sync_states_guard.truncate(1);
                    let first = sync_states_guard.first_mut().unwrap();
                    first.set_epoch(heaviest.epoch());
                    first.set_target(state_machine.lock().heaviest_tipset());
                    let seconds_per_epoch = state_manager.chain_config().block_delay_secs;
                    let time_diff =
                        (Utc::now().timestamp() as u64).saturating_sub(heaviest.min_timestamp());
                    if time_diff < seconds_per_epoch as u64 * 2 {
                        first.set_stage(SyncStage::Complete);
                    } else {
                        first.set_stage(SyncStage::Messages);
                    }
                    sync_states_guard.extend(states);
                }

                for task in task_vec {
                    // insert task into tasks. If task is already in tasks, skip. If it is not, spawn it.
                    let new = tasks_set.insert(task.clone());
                    if new {
                        // info!("Spawning task: {}", task);
                        let tasks_clone = tasks.clone();
                        let action = task.clone().execute(
                            network.clone(),
                            state_manager.clone(),
                            bad_block_cache.clone(),
                        );
                        tokio::spawn({
                            let state_machine = state_machine.clone();
                            let state_changed = state_changed.clone();
                            async move {
                                if let Some(event) = action.await {
                                    state_machine.lock().update(event);
                                    state_changed.notify_one();
                                }
                                tasks_clone.lock().remove(&task);
                            }
                        });
                    }
                }
            }
        }
    });

    // Add status reporting task
    set.spawn({
        let state_manager = state_manager.clone();
        let state_machine = state_machine.clone();
        async move {
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
                let (tasks_set, _) = state_machine.lock().tasks();
                let heaviest_epoch = state_manager.chain_store().heaviest_tipset().epoch();
                let fork_cutoff = heaviest_epoch
                    - SECONDS_IN_DAY / (state_manager.chain_config().block_delay_secs as i64);

                // Count tipsets to fetch for main chain and forks
                let mut main_chain_epochs = Vec::new();
                let mut fork_tipsets = 0;

                for task in tasks_set.iter() {
                    if let SyncTask::FetchTipset(_, epoch) = task {
                        let diff = epoch - heaviest_epoch;
                        if diff >= 0 {
                            main_chain_epochs.push(diff);
                        } else {
                            // This is a fork - we'll download a fixed number of tipsets
                            fork_tipsets = fork_tipsets.max(epoch - fork_cutoff);
                        }
                    }
                }

                // Sort epochs for consistent display
                main_chain_epochs.sort();

                match (!main_chain_epochs.is_empty(), fork_tipsets > 0) {
                    (true, true) => info!(
                        "Fetching tipsets: {}, Forks: {} tipsets to fetch",
                        main_chain_epochs
                            .iter()
                            .map(|e| e.to_string())
                            .collect::<Vec<_>>()
                            .join(", "),
                        fork_tipsets
                    ),
                    (true, false) => info!(
                        "Fetching tipsets: {}",
                        main_chain_epochs
                            .iter()
                            .map(|e| e.to_string())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                    (false, true) => {
                        info!("Fetching tipsets: Forks: {} tipsets to fetch", fork_tipsets)
                    }
                    (false, false) => {}
                }
            }
        }
    });

    set.join_all().await;
    Ok(())
}

// Increment the gossipsub event metrics.
fn inc_gossipsub_event_metrics(event: &NetworkEvent) {
    let label = match event {
        NetworkEvent::HelloRequestInbound => metrics::values::HELLO_REQUEST_INBOUND,
        NetworkEvent::HelloResponseOutbound { .. } => metrics::values::HELLO_RESPONSE_OUTBOUND,
        NetworkEvent::HelloRequestOutbound => metrics::values::HELLO_REQUEST_OUTBOUND,
        NetworkEvent::HelloResponseInbound => metrics::values::HELLO_RESPONSE_INBOUND,
        NetworkEvent::PeerConnected(_) => metrics::values::PEER_CONNECTED,
        NetworkEvent::PeerDisconnected(_) => metrics::values::PEER_DISCONNECTED,
        NetworkEvent::PubsubMessage { message } => match message {
            PubsubMessage::Block(_) => metrics::values::PUBSUB_BLOCK,
            PubsubMessage::Message(_) => metrics::values::PUBSUB_MESSAGE,
        },
        NetworkEvent::ChainExchangeRequestOutbound => {
            metrics::values::CHAIN_EXCHANGE_REQUEST_OUTBOUND
        }
        NetworkEvent::ChainExchangeResponseInbound => {
            metrics::values::CHAIN_EXCHANGE_RESPONSE_INBOUND
        }
        NetworkEvent::ChainExchangeRequestInbound => {
            metrics::values::CHAIN_EXCHANGE_REQUEST_INBOUND
        }
        NetworkEvent::ChainExchangeResponseOutbound => {
            metrics::values::CHAIN_EXCHANGE_RESPONSE_OUTBOUND
        }
    };

    metrics::LIBP2P_MESSAGE_TOTAL.get_or_create(&label).inc();
}

// Keep our peer manager up to date.
fn upd_peer_information<DB: Blockstore + Sync + Send + 'static>(
    event: &NetworkEvent,
    network: SyncNetworkContext<DB>,
    chain_store: Arc<ChainStore<DB>>,
    genesis: &Tipset,
) {
    match event {
        NetworkEvent::PeerConnected(peer_id) => {
            let genesis_cid = *genesis.block_headers().first().cid();
            // Spawn and immediately move on to the next event
            tokio::task::spawn(handle_peer_connected_event(
                network,
                chain_store,
                *peer_id,
                genesis_cid,
            ));
        }
        NetworkEvent::PeerDisconnected(peer_id) => {
            handle_peer_disconnected_event(network, *peer_id);
        }
        _ => {}
    }
}

async fn handle_peer_connected_event<DB: Blockstore + Sync + Send + 'static>(
    network: SyncNetworkContext<DB>,
    chain_store: Arc<ChainStore<DB>>,
    peer_id: PeerId,
    genesis_block_cid: Cid,
) {
    // Query the heaviest TipSet from the store
    if network.peer_manager().is_peer_new(&peer_id) {
        // Since the peer is new, send them a hello request
        // Query the heaviest TipSet from the store
        let heaviest = chain_store.heaviest_tipset();
        let request = HelloRequest {
            heaviest_tip_set: heaviest.cids(),
            heaviest_tipset_height: heaviest.epoch(),
            heaviest_tipset_weight: heaviest.weight().clone().into(),
            genesis_cid: genesis_block_cid,
        };
        let (peer_id, moment_sent, response) = match network.hello_request(peer_id, request).await {
            Ok(response) => response,
            Err(e) => {
                debug!("Hello request failed: {}", e);
                return;
            }
        };
        let dur = SystemTime::now()
            .duration_since(moment_sent)
            .unwrap_or_default();

        // Update the peer metadata based on the response
        match response {
            Some(_) => {
                network.peer_manager().log_success(&peer_id, dur);
            }
            None => {
                network.peer_manager().log_failure(&peer_id, dur);
            }
        }
    }
}

fn handle_peer_disconnected_event<DB: Blockstore + Sync + Send + 'static>(
    network: SyncNetworkContext<DB>,
    peer_id: PeerId,
) {
    network.peer_manager().remove_peer(&peer_id);
    network.peer_manager().unmark_peer_bad(&peer_id);
}

async fn get_full_tipset<DB: Blockstore + Sync + Send + 'static>(
    network: SyncNetworkContext<DB>,
    chain_store: Arc<ChainStore<DB>>,
    peer_id: Option<PeerId>,
    tipset_keys: TipsetKey,
) -> anyhow::Result<FullTipset> {
    // Attempt to load from the store
    if let Ok(full_tipset) = load_full_tipset(&chain_store, tipset_keys.clone()) {
        return Ok(full_tipset);
    }
    // Load from the network
    let tipset = network
        .chain_exchange_fts(peer_id, &tipset_keys.clone())
        .await
        .map_err(|e| anyhow::anyhow!(e))?;

    for block in tipset.blocks() {
        block.persist(&chain_store.db)?;
        crate::chain::persist_objects(&chain_store.db, block.bls_messages.iter())?;
        crate::chain::persist_objects(&chain_store.db, block.secp_messages.iter())?;
    }

    // This is needed for the Ethereum mapping
    chain_store.put_tipset_key(tipset.key())?;

    Ok(tipset)
}

async fn get_full_tipset_batch<DB: Blockstore + Sync + Send + 'static>(
    network: SyncNetworkContext<DB>,
    chain_store: Arc<ChainStore<DB>>,
    peer_id: Option<PeerId>,
    tipset_keys: TipsetKey,
) -> anyhow::Result<Vec<FullTipset>> {
    // Attempt to load from the store
    if let Ok(full_tipset) = load_full_tipset(&chain_store, tipset_keys.clone()) {
        return Ok(vec![full_tipset]);
    }
    // Load from the network
    let tipsets = network
        .chain_exchange_full_tipsets(peer_id, &tipset_keys.clone())
        .await
        .map_err(|e| anyhow::anyhow!(e))?;

    for tipset in tipsets.iter() {
        for block in tipset.blocks() {
            block.persist(&chain_store.db)?;
            crate::chain::persist_objects(&chain_store.db, block.bls_messages.iter())?;
            crate::chain::persist_objects(&chain_store.db, block.secp_messages.iter())?;
        }
        // This is needed for the Ethereum mapping
        chain_store.put_tipset_key(tipset.key())?;
    }

    Ok(tipsets)
}

fn load_full_tipset<DB: Blockstore>(
    chain_store: &ChainStore<DB>,
    tipset_keys: TipsetKey,
) -> anyhow::Result<FullTipset> {
    // Retrieve tipset from store based on passed in TipsetKey
    let ts = chain_store.chain_index.load_required_tipset(&tipset_keys)?;

    let blocks: Vec<_> = ts
        .block_headers()
        .iter()
        .map(|header| -> anyhow::Result<Block> {
            let (bls_msgs, secp_msgs) =
                crate::chain::block_messages(chain_store.blockstore(), header)?;
            Ok(Block {
                header: header.clone(),
                bls_messages: bls_msgs,
                secp_messages: secp_msgs,
            })
        })
        .try_collect()?;

    // Construct FullTipset
    let fts = FullTipset::new(blocks)?;
    Ok(fts)
}

enum SyncEvent {
    NewFullTipsets(Vec<Arc<FullTipset>>),
    BadTipset(Arc<FullTipset>, String),
    ValidatedTipset(Arc<FullTipset>),
}

struct SyncStateMachine<DB> {
    chain_config: Arc<ChainConfig>,
    cs: Arc<ChainStore<DB>>,
    bad_block_cache: Arc<BadBlockCache>,
    // Map from TipsetKey to FullTipset
    tipsets: HashMap<TipsetKey, Arc<FullTipset>>,
}

impl<DB: Blockstore> SyncStateMachine<DB> {
    pub fn new(
        cs: Arc<ChainStore<DB>>,
        chain_config: Arc<ChainConfig>,
        bad_block_cache: Arc<BadBlockCache>,
    ) -> Self {
        Self {
            cs,
            chain_config,
            bad_block_cache,
            tipsets: HashMap::default(),
        }
    }

    // Compute the list of chains from the tipsets map
    fn chains(&self) -> Vec<Vec<Arc<FullTipset>>> {
        let mut chains = Vec::new();
        let mut remaining_tipsets = self.tipsets.clone();

        while let Some(heaviest) = remaining_tipsets
            .values()
            .max_by_key(|ts| ts.weight())
            .cloned()
        {
            // Build chain starting from heaviest
            let mut chain = Vec::new();
            let mut current = Some(heaviest);

            while let Some(tipset) = current {
                chain.push(tipset.clone());
                remaining_tipsets.remove(tipset.key());

                // Find parent in remaining tipsets
                current = remaining_tipsets.get(tipset.parents()).cloned();
            }
            chain.reverse();
            chains.push(chain);
        }

        chains
    }

    fn heaviest_tipset(&self) -> Option<Arc<Tipset>> {
        self.tipsets
            .values()
            .max_by_key(|ts| ts.weight())
            .map(|ts| Arc::new(ts.deref().clone().into_tipset()))
    }

    fn is_validated(&self, tipset: &FullTipset) -> bool {
        let db = self.cs.blockstore();
        db.has(tipset.parent_state()).unwrap_or(false)
    }

    fn is_ready_for_validation(&self, tipset: &FullTipset) -> bool {
        if let Ok(full_tipset) = load_full_tipset(&self.cs, tipset.parents().clone()) {
            self.is_validated(&full_tipset)
        } else {
            false
        }
    }

    fn add_full_tipset(&mut self, tipset: Arc<FullTipset>) {
        if let Err(why) = TipsetValidator(&tipset).validate(
            &self.cs,
            Some(&self.bad_block_cache),
            &self.cs.genesis_tipset(),
            self.chain_config.block_delay_secs,
        ) {
            metrics::INVALID_TIPSET_TOTAL.inc();
            trace!("Skipping invalid tipset: {}", why);
            self.mark_bad_tipset(tipset, why.to_string());
            return;
        }

        // Check if tipset is older than a day compared to heaviest tipset
        let heaviest = self.cs.heaviest_tipset();
        let epoch_diff = heaviest.epoch() - tipset.epoch();
        let time_diff = epoch_diff * (self.chain_config.block_delay_secs as i64);

        if time_diff > SECONDS_IN_DAY {
            // info!(
            //     "Add tipset: Ignoring old tipset. epoch: {}, heaviest: {}, diff: {}s",
            //     tipset.epoch(),
            //     heaviest.epoch(),
            //     time_diff
            // );
            self.mark_bad_tipset(tipset, "old tipset".to_string());
            return;
        }

        // if self.is_validated(&tipset) {
        //     info!("Add tipset: Already validated. epoch: {:?}", tipset.epoch());
        //     return;
        // }

        // Check if tipset already exists
        if self.tipsets.contains_key(tipset.key()) {
            // info!("Add tipset: Already in map. epoch: {:?}", tipset.epoch());
            return;
        }

        // Find any existing tipsets with same epoch and parents
        let mut to_remove = Vec::new();
        #[allow(clippy::mutable_key_type)]
        let mut merged_blocks: HashSet<_> = tipset.blocks().iter().cloned().collect();

        // Collect all parent references from existing tipsets
        let parent_refs: HashSet<_> = self
            .tipsets
            .values()
            .map(|ts| ts.parents().clone())
            .collect();

        for (key, existing_ts) in self.tipsets.iter() {
            if existing_ts.epoch() == tipset.epoch() && existing_ts.parents() == tipset.parents() {
                // Only mark for removal if not referenced as a parent
                if !parent_refs.contains(key) {
                    to_remove.push(key.clone());
                }
                // Add blocks from existing tipset - HashSet handles deduplication automatically
                merged_blocks.extend(existing_ts.blocks().iter().cloned());
            }
        }

        // Remove old tipsets that were merged and aren't referenced
        for key in to_remove {
            self.tipsets.remove(&key);
        }

        // Create and insert new merged tipset
        if let Ok(merged_tipset) = FullTipset::new(merged_blocks) {
            self.tipsets
                .insert(merged_tipset.key().clone(), Arc::new(merged_tipset));
        }
    }

    // Mark blocks in tipset as bad.
    // Mark all descendants of tipsets as bad.
    // Remove all bad tipsets from the tipset map.
    fn mark_bad_tipset(&mut self, tipset: Arc<FullTipset>, reason: String) {
        self.tipsets.remove(tipset.key());
        // Mark all blocks in the tipset as bad
        for block in tipset.blocks() {
            self.bad_block_cache.put(*block.cid(), reason.clone());
        }

        // Find all descendant tipsets (tipsets that have this tipset as a parent)
        let mut to_remove = Vec::new();
        let mut descendants = Vec::new();

        for (key, ts) in self.tipsets.iter() {
            if ts.parents() == tipset.key() {
                to_remove.push(key.clone());
                descendants.push(ts.clone());
            }
        }

        // Remove bad tipsets from the map
        for key in to_remove {
            self.tipsets.remove(&key);
        }

        // Recursively mark descendants as bad
        for descendant in descendants {
            self.mark_bad_tipset(descendant, reason.clone());
        }
    }

    fn mark_validated_tipset(&mut self, tipset: Arc<FullTipset>) {
        // FIXME: Should navigate to the heaviest tipset in the chain
        assert!(self.is_validated(&tipset), "Tipset must be validated");
        self.tipsets.remove(tipset.key());
        let tipset = tipset.deref().clone().into_tipset();
        let _ = self.cs.put_tipset(&tipset);
    }

    pub fn update(&mut self, event: SyncEvent) {
        match event {
            SyncEvent::NewFullTipsets(tipsets) => {
                for tipset in tipsets {
                    self.add_full_tipset(tipset);
                }
            }
            SyncEvent::BadTipset(tipset, reason) => self.mark_bad_tipset(tipset, reason),
            SyncEvent::ValidatedTipset(tipset) => self.mark_validated_tipset(tipset),
        }
    }

    pub fn tasks(&self) -> (Vec<SyncTask>, Vec<SyncState>) {
        let mut states = Vec::new();
        let mut tasks = Vec::new();
        for chain in self.chains() {
            if let Some(first_ts) = chain.first() {
                let last = chain.last().expect("Infallible");
                let mut state = SyncState::default();
                state.init(
                    Arc::new(first_ts.deref().clone().into_tipset()),
                    Arc::new(last.deref().clone().into_tipset()),
                );
                state.set_epoch(first_ts.epoch());
                if !self.is_ready_for_validation(first_ts) {
                    state.set_stage(SyncStage::Headers);
                    tasks.push(SyncTask::FetchTipset(
                        first_ts.parents().clone(),
                        first_ts.epoch(),
                    ));
                } else {
                    if last.epoch() - first_ts.epoch() > 5 {
                        state.set_stage(SyncStage::Messages);
                    } else {
                        state.set_stage(SyncStage::Complete);
                    }
                    tasks.push(SyncTask::ValidateTipset(first_ts.clone()));
                }
                states.push(state);
            }
        }
        (tasks, states)
    }
}

#[derive(PartialEq, Eq, Hash, Clone, Debug)]
enum SyncTask {
    ValidateTipset(Arc<FullTipset>),
    FetchTipset(TipsetKey, ChainEpoch),
}

impl std::fmt::Display for SyncTask {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SyncTask::ValidateTipset(ts) => write!(f, "ValidateTipset(epoch: {})", ts.epoch()),
            SyncTask::FetchTipset(key, epoch) => {
                let s = key.to_string();
                write!(
                    f,
                    "FetchTipset({}, epoch: {})",
                    &s[s.len().saturating_sub(8)..],
                    epoch
                )
            }
        }
    }
}

impl SyncTask {
    async fn execute<DB: Blockstore + Sync + Send + 'static>(
        self,
        network: SyncNetworkContext<DB>,
        state_manager: Arc<StateManager<DB>>,
        bad_block_cache: Arc<BadBlockCache>,
    ) -> Option<SyncEvent> {
        let cs = state_manager.chain_store();
        match self {
            SyncTask::ValidateTipset(tipset) => {
                let genesis = cs.genesis_tipset();
                match validate_tipset(
                    state_manager.clone(),
                    cs,
                    &bad_block_cache,
                    tipset.deref().clone(),
                    &genesis,
                    InvalidBlockStrategy::Forgiving,
                )
                .await
                {
                    Ok(()) => {
                        let _ = cs.put_delegated_message_hashes(
                            tipset.blocks().iter().map(|b| b.header()),
                        );
                        Some(SyncEvent::ValidatedTipset(tipset))
                    }
                    Err(e) => {
                        warn!("Error validating tipset: {}", e);
                        Some(SyncEvent::BadTipset(tipset, e.to_string()))
                    }
                }
            }
            SyncTask::FetchTipset(key, _epoch) => {
                if let Some(reason) = bad_block_cache.peek_tipset_key(&key) {
                    debug!("Skipping fetch of bad tipset: {}", reason);
                    return None;
                }
                if let Ok(parents) =
                    get_full_tipset_batch(network.clone(), cs.clone(), None, key).await
                {
                    Some(SyncEvent::NewFullTipsets(
                        parents.into_iter().map(Arc::new).collect(),
                    ))
                } else {
                    None
                }
            }
        }
    }
}
