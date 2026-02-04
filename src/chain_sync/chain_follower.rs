// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
//! This module contains the logic for driving Forest forward in the Filecoin
//! blockchain.
//!
//! Forest keeps track of the current heaviest tipset, and receives information
//! about new blocks and tipsets from peers as well as connected miners. The
//! state machine has the following rules:
//! - A tipset is invalid if its parent is invalid.
//! - If a tipset's parent isn't in our database, request it from the network.
//! - If a tipset's parent has been validated, validate the tipset.
//! - If a tipset is 1 day older than the heaviest tipset, the tipset is
//!   invalid. This prevents Forest from following forks that will never be
//!   accepted.
//!
//! The state machine does not do any network requests or validation. Those are
//! handled by an external actor.

use super::network_context::SyncNetworkContext;
use crate::{
    blocks::{Block, FullTipset, Tipset, TipsetKey},
    chain::ChainStore,
    chain_sync::{
        ForkSyncInfo, ForkSyncStage, SyncStatus, SyncStatusReport, TipsetValidator,
        bad_block_cache::BadBlockCache, metrics, tipset_syncer::validate_tipset,
    },
    libp2p::{NetworkEvent, PubsubMessage, hello::HelloRequest},
    message_pool::{MessagePool, MpoolRpcProvider},
    networks::calculate_expected_epoch,
    shim::clock::ChainEpoch,
    state_manager::StateManager,
    utils::misc::env::is_env_truthy,
};
use ahash::{HashMap, HashSet};
use chrono::Utc;
use cid::Cid;
use fvm_ipld_blockstore::Blockstore;
use itertools::Itertools;
use libp2p::PeerId;
use parking_lot::{Mutex, RwLock};
use std::{sync::Arc, time::Instant};
use tokio::{sync::Notify, task::JoinSet};
use tracing::{debug, error, info, trace, warn};

pub struct ChainFollower<DB> {
    /// Syncing status of the chain
    pub sync_status: SyncStatus,

    /// manages retrieving and updates state objects
    state_manager: Arc<StateManager<DB>>,

    /// Context to be able to send requests to P2P network
    pub network: SyncNetworkContext<DB>,

    /// Genesis tipset
    genesis: Tipset,

    /// Bad blocks cache, updates based on invalid state transitions.
    /// Will mark any invalid blocks and all children as bad in this bounded
    /// cache
    pub bad_blocks: Option<Arc<BadBlockCache>>,

    /// Incoming network events to be handled by synchronizer
    net_handler: flume::Receiver<NetworkEvent>,

    /// Tipset channel sender
    pub tipset_sender: flume::Sender<FullTipset>,

    /// Tipset channel receiver
    tipset_receiver: flume::Receiver<FullTipset>,

    /// When `stateless_mode` is true, forest connects to the P2P network but
    /// does not execute any state transitions. This drastically reduces the
    /// memory and disk footprint of Forest but also means that Forest will not
    /// be able to validate the correctness of the chain.
    stateless_mode: bool,

    /// Message pool
    mem_pool: Arc<MessagePool<MpoolRpcProvider<DB>>>,
}

impl<DB: Blockstore + Sync + Send + 'static> ChainFollower<DB> {
    pub fn new(
        state_manager: Arc<StateManager<DB>>,
        network: SyncNetworkContext<DB>,
        genesis: Tipset,
        net_handler: flume::Receiver<NetworkEvent>,
        stateless_mode: bool,
        mem_pool: Arc<MessagePool<MpoolRpcProvider<DB>>>,
    ) -> Self {
        let (tipset_sender, tipset_receiver) = flume::bounded(20);
        let disable_bad_block_cache = is_env_truthy("FOREST_DISABLE_BAD_BLOCK_CACHE");
        Self {
            sync_status: Arc::new(RwLock::new(SyncStatusReport::init())),
            state_manager,
            network,
            genesis,
            bad_blocks: if disable_bad_block_cache {
                tracing::warn!("bad block cache is disabled by `FOREST_DISABLE_BAD_BLOCK_CACHE`");
                None
            } else {
                Some(Default::default())
            },
            net_handler,
            tipset_sender,
            tipset_receiver,
            stateless_mode,
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
            self.sync_status,
            self.genesis,
            self.stateless_mode,
        )
        .await
    }
}

#[allow(clippy::too_many_arguments)]
// We receive new full tipsets from the p2p swarm, and from miners that use Forest as their frontend.
pub async fn chain_follower<DB: Blockstore + Sync + Send + 'static>(
    state_manager: Arc<StateManager<DB>>,
    bad_block_cache: Option<Arc<BadBlockCache>>,
    network_rx: flume::Receiver<NetworkEvent>,
    tipset_receiver: flume::Receiver<FullTipset>,
    network: SyncNetworkContext<DB>,
    mem_pool: Arc<MessagePool<MpoolRpcProvider<DB>>>,
    sync_status: SyncStatus,
    genesis: Tipset,
    stateless_mode: bool,
) -> anyhow::Result<()> {
    let state_changed = Arc::new(Notify::new());
    let state_machine = Arc::new(Mutex::new(SyncStateMachine::new(
        state_manager.chain_store().clone(),
        bad_block_cache.clone(),
        stateless_mode,
    )));
    let tasks: Arc<Mutex<HashSet<SyncTask>>> = Arc::new(Mutex::new(HashSet::default()));

    let mut set = JoinSet::new();

    // Increment metrics, update peer information, and forward tipsets to the state machine.
    set.spawn({
        let state_manager = state_manager.clone();
        let state_changed = state_changed.clone();
        let state_machine = state_machine.clone();
        let network = network.clone();
        async move {
            while let Ok(event) = network_rx.recv_async().await {
                inc_gossipsub_event_metrics(&event);

                update_peer_info(
                    &event,
                    &network,
                    state_manager.chain_store().clone(),
                    &genesis,
                );

                let Ok(tipset) = (match event {
                    NetworkEvent::HelloResponseOutbound { request, source } => {
                        let tipset_keys = TipsetKey::from(request.heaviest_tip_set.clone());
                        get_full_tipset(
                            &network,
                            state_manager.chain_store(),
                            Some(source),
                            &tipset_keys,
                        )
                        .await
                        .inspect_err(|e| debug!("Querying full tipset failed: {}", e))
                    }
                    NetworkEvent::PubsubMessage { message } => match message {
                        PubsubMessage::Block(b) => {
                            let key = TipsetKey::from(nunny::vec![*b.header.cid()]);
                            get_full_tipset(&network, state_manager.chain_store(), None, &key).await
                        }
                        PubsubMessage::Message(m) => {
                            if let Err(why) = mem_pool.add(m) {
                                debug!("Received invalid GossipSub message: {}", why);
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
                        .update(SyncEvent::NewFullTipsets(vec![tipset]));
                    state_changed.notify_one();
                }
            }
        }
    });

    // Forward tipsets from miners into the state machine.
    set.spawn({
        let state_changed = state_changed.clone();
        let state_machine = state_machine.clone();

        async move {
            while let Ok(tipset) = tipset_receiver.recv_async().await {
                state_machine
                    .lock()
                    .update(SyncEvent::NewFullTipsets(vec![tipset]));
                state_changed.notify_one();
            }
        }
    });

    // When the state machine is updated, we need to update the sync status and spawn tasks
    set.spawn({
        let state_manager = state_manager.clone();
        let state_machine = state_machine.clone();
        let state_changed = state_changed.clone();
        let tasks = tasks.clone();
        let bad_block_cache = bad_block_cache.clone();
        async move {
            loop {
                state_changed.notified().await;

                let mut tasks_set = tasks.lock();
                let (task_vec, current_active_forks) = state_machine.lock().tasks();

                // Update the sync states
                {
                    let mut status_report_guard = sync_status.write();
                    status_report_guard.update(
                        &state_manager,
                        current_active_forks,
                        stateless_mode,
                    );
                }

                for task in task_vec {
                    // insert task into tasks. If task is already in tasks, skip. If it is not, spawn it.
                    let new = tasks_set.insert(task.clone());
                    if new {
                        let action = task.clone().execute(
                            network.clone(),
                            state_manager.clone(),
                            stateless_mode,
                            bad_block_cache.clone(),
                        );
                        tokio::spawn({
                            let tasks = tasks.clone();
                            let state_machine = state_machine.clone();
                            let state_changed = state_changed.clone();
                            async move {
                                if let Some(event) = action.await {
                                    state_machine.lock().update(event);
                                    state_changed.notify_one();
                                }
                                tasks.lock().remove(&task);
                            }
                        });
                    }
                }
            }
        }
    });

    // Periodically report progress if there are any tipsets left to be fetched.
    // Once we're in steady-state (i.e. caught up to HEAD) and there are no
    // active forks, this will not report anything.
    set.spawn({
        let state_manager = state_manager.clone();
        let state_machine = state_machine.clone();
        async move {
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
                let (tasks_set, _) = state_machine.lock().tasks();
                let heaviest_tipset = state_manager.chain_store().heaviest_tipset();
                let heaviest_epoch = heaviest_tipset.epoch();

                let to_download = tasks_set
                    .iter()
                    .filter_map(|task| match task {
                        SyncTask::FetchTipset(_, epoch) => Some(epoch - heaviest_epoch),
                        _ => None,
                    })
                    .max()
                    .unwrap_or(0);

                let expected_head = calculate_expected_epoch(
                    Utc::now().timestamp() as u64,
                    state_manager.chain_store().genesis_block_header().timestamp,
                    state_manager.chain_config().block_delay_secs,
                );

                // Only print 'Catching up to HEAD' if we're more than 10 epochs
                // behind. Otherwise it can be too spammy.
                match (expected_head - heaviest_epoch > 10, to_download > 0) {
                    (true, true) => info!(
                        "Catching up to HEAD: {heaviest_epoch}{} -> {expected_head}, downloading {to_download} tipsets"
                        , heaviest_tipset.key()
                    ),
                    (true, false) => info!(
                        "Catching up to HEAD: {heaviest_epoch}{} -> {expected_head}"
                        , heaviest_tipset.key()
                    ),
                    (false, true) => {
                        info!("Downloading {to_download} tipsets")
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
fn update_peer_info<DB: Blockstore + Sync + Send + 'static>(
    event: &NetworkEvent,
    network: &SyncNetworkContext<DB>,
    chain_store: Arc<ChainStore<DB>>,
    genesis: &Tipset,
) {
    match event {
        NetworkEvent::PeerConnected(peer_id) => {
            let genesis_cid = *genesis.block_headers().first().cid();
            // Spawn and immediately move on to the next event
            tokio::task::spawn(handle_peer_connected_event(
                network.clone(),
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
        let dur = Instant::now().duration_since(moment_sent);

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
    network: &SyncNetworkContext<DB>,
    peer_id: PeerId,
) {
    network.peer_manager().remove_peer(&peer_id);
    network.peer_manager().unmark_peer_bad(&peer_id);
}

pub async fn get_full_tipset<DB: Blockstore + Sync + Send + 'static>(
    network: &SyncNetworkContext<DB>,
    chain_store: &ChainStore<DB>,
    peer_id: Option<PeerId>,
    tipset_keys: &TipsetKey,
) -> anyhow::Result<FullTipset> {
    // Attempt to load from the store
    if let Ok(full_tipset) = load_full_tipset(chain_store, tipset_keys) {
        return Ok(full_tipset);
    }
    // Load from the network
    let tipset = network
        .chain_exchange_full_tipset(peer_id, tipset_keys)
        .await
        .map_err(|e| anyhow::anyhow!(e))?;
    tipset.persist(chain_store.blockstore())?;

    Ok(tipset)
}

async fn get_full_tipset_batch<DB: Blockstore + Sync + Send + 'static>(
    network: &SyncNetworkContext<DB>,
    chain_store: &ChainStore<DB>,
    peer_id: Option<PeerId>,
    tipset_keys: &TipsetKey,
) -> anyhow::Result<Vec<FullTipset>> {
    // Attempt to load from the store
    if let Ok(full_tipset) = load_full_tipset(chain_store, tipset_keys) {
        return Ok(vec![full_tipset]);
    }
    // Load from the network
    let tipsets = network
        .chain_exchange_full_tipsets(peer_id, tipset_keys)
        .await
        .map_err(|e| anyhow::anyhow!(e))?;

    for tipset in tipsets.iter() {
        tipset.persist(chain_store.blockstore())?;
    }

    Ok(tipsets)
}

pub fn load_full_tipset<DB: Blockstore>(
    chain_store: &ChainStore<DB>,
    tipset_keys: &TipsetKey,
) -> anyhow::Result<FullTipset> {
    // Retrieve tipset from store based on passed in TipsetKey
    let ts = chain_store
        .chain_index()
        .load_required_tipset(tipset_keys)?;
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
    NewFullTipsets(Vec<FullTipset>),
    BadTipset(FullTipset),
    ValidatedTipset {
        tipset: FullTipset,
        is_proposed_head: bool,
    },
}

impl std::fmt::Display for SyncEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        fn tss_to_string(tss: &[FullTipset]) -> String {
            format!(
                "epoch: {}-{}",
                tss.first().map(|ts| ts.epoch()).unwrap_or_default(),
                tss.last().map(|ts| ts.epoch()).unwrap_or_default()
            )
        }

        match self {
            Self::NewFullTipsets(tss) => write!(f, "NewFullTipsets({})", tss_to_string(tss)),
            Self::BadTipset(ts) => {
                write!(f, "BadTipset(epoch: {}, key: {})", ts.epoch(), ts.key())
            }
            Self::ValidatedTipset {
                tipset,
                is_proposed_head,
            } => write!(
                f,
                "ValidatedTipset(epoch: {}, key: {}, is_proposed_head: {is_proposed_head})",
                tipset.epoch(),
                tipset.key()
            ),
        }
    }
}

struct SyncStateMachine<DB> {
    cs: Arc<ChainStore<DB>>,
    bad_block_cache: Option<Arc<BadBlockCache>>,
    // Map from TipsetKey to FullTipset
    tipsets: HashMap<TipsetKey, FullTipset>,
    stateless_mode: bool,
}

impl<DB: Blockstore> SyncStateMachine<DB> {
    pub fn new(
        cs: Arc<ChainStore<DB>>,
        bad_block_cache: Option<Arc<BadBlockCache>>,
        stateless_mode: bool,
    ) -> Self {
        Self {
            cs,
            bad_block_cache,
            tipsets: HashMap::default(),
            stateless_mode,
        }
    }

    // Compute the list of chains from the tipsets map
    fn chains(&self) -> Vec<Vec<FullTipset>> {
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

            while let Some(tipset) = current.take() {
                remaining_tipsets.remove(tipset.key());

                // Find parent in tipsets map
                current = self.tipsets.get(tipset.parents()).cloned();

                chain.push(tipset);
            }
            chain.reverse();
            chains.push(chain);
        }

        chains
    }

    fn is_parent_validated(&self, tipset: &FullTipset) -> bool {
        let db = self.cs.blockstore();
        self.stateless_mode || db.has(tipset.parent_state()).unwrap_or(false)
    }

    fn is_ready_for_validation(&self, tipset: &FullTipset) -> bool {
        if self.stateless_mode || tipset.key() == self.cs.genesis_tipset().key() {
            // Skip validation in stateless mode and for genesis tipset
            true
        } else if let Ok(parent_ts) = load_full_tipset(&self.cs, tipset.parents()) {
            let head_ts = self.cs.heaviest_tipset();
            // Treat post-head-epoch tipsets as not validated to fix <https://github.com/ChainSafe/forest/issues/5677>
            // basically, the follow task should always start from the current head which could be manually set
            // to an old one. When a post-head-epoch tipset is considered validated, it could mess up the state machine
            // in some edge cases and the node ends up being stuck with ever-empty sync task queue as reported
            // in <https://github.com/ChainSafe/forest/issues/5679>.
            if parent_ts.key() == head_ts.key() {
                true
            } else if parent_ts.epoch() >= head_ts.epoch() {
                false
            } else {
                self.is_parent_validated(tipset)
            }
        } else {
            false
        }
    }

    fn add_full_tipset(&mut self, tipset: FullTipset) {
        if let Err(why) = TipsetValidator(&tipset).validate(
            &self.cs,
            self.bad_block_cache.as_ref().map(AsRef::as_ref),
            &self.cs.genesis_tipset(),
            self.cs.chain_config().block_delay_secs,
        ) {
            metrics::INVALID_TIPSET_TOTAL.inc();
            trace!("Skipping invalid tipset: {}", why);
            self.mark_bad_tipset(tipset);
            return;
        }

        // Check if tipset is outside the chain_finality window
        let heaviest = self.cs.heaviest_tipset();
        let epoch_diff = heaviest.epoch() - tipset.epoch();

        if epoch_diff > self.cs.chain_config().policy.chain_finality {
            self.mark_bad_tipset(tipset);
            return;
        }

        // Check if tipset already exists
        if self.tipsets.contains_key(tipset.key()) {
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
                .insert(merged_tipset.key().clone(), merged_tipset);
        }
    }

    // Mark blocks in tipset as bad.
    // Mark all descendants of tipsets as bad.
    // Remove all bad tipsets from the tipset map.
    fn mark_bad_tipset(&mut self, tipset: FullTipset) {
        let mut stack = vec![tipset];
        while let Some(tipset) = stack.pop() {
            self.tipsets.remove(tipset.key());
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

            // Mark descendants as bad
            stack.extend(descendants);
        }
    }

    fn mark_validated_tipset(&mut self, tipset: FullTipset, is_proposed_head: bool) {
        if !self.is_parent_validated(&tipset) {
            tracing::error!(epoch = %tipset.epoch(), tsk = %tipset.key(), "Tipset must be validated");
            return;
        }

        self.tipsets.remove(tipset.key());
        let tipset = tipset.into_tipset();
        // cs.put_tipset requires state and doesn't work in stateless mode
        if self.stateless_mode {
            let epoch = tipset.epoch();
            let terse_key = tipset.key().terse();
            if self.cs.heaviest_tipset().weight() < tipset.weight() {
                if let Err(e) = self.cs.set_heaviest_tipset(tipset) {
                    error!("Error setting heaviest tipset: {}", e);
                } else {
                    info!("Heaviest tipset: {} ({})", epoch, terse_key);
                }
            }
        } else if is_proposed_head {
            if let Err(e) = self.cs.put_tipset(&tipset) {
                error!("Error putting tipset: {e}");
            }
        } else if let Err(e) = self.cs.set_heaviest_tipset(tipset) {
            error!("Error setting heaviest tipset: {e}");
        }
    }

    pub fn update(&mut self, event: SyncEvent) {
        tracing::trace!("update: {event}");
        match event {
            SyncEvent::NewFullTipsets(tipsets) => {
                for tipset in tipsets {
                    self.add_full_tipset(tipset);
                }
            }
            SyncEvent::BadTipset(tipset) => self.mark_bad_tipset(tipset),
            SyncEvent::ValidatedTipset {
                tipset,
                is_proposed_head,
            } => self.mark_validated_tipset(tipset, is_proposed_head),
        }
    }

    pub fn tasks(&self) -> (Vec<SyncTask>, Vec<ForkSyncInfo>) {
        // Get the node's current validated head epoch once, as it's the same for all forks.
        let current_validated_epoch = self.cs.heaviest_tipset().epoch();
        let now = Utc::now();

        let mut active_sync_info = Vec::new();
        let mut tasks = Vec::new();
        for chain in self.chains() {
            if let Some(first_ts) = chain.first() {
                let last_ts = chain.last().expect("Infallible");
                let stage: ForkSyncStage;
                let start_time = Some(now);

                if !self.is_ready_for_validation(first_ts) {
                    stage = ForkSyncStage::FetchingHeaders;
                    tasks.push(SyncTask::FetchTipset(
                        first_ts.parents().clone(),
                        first_ts.epoch(),
                    ));
                } else {
                    stage = ForkSyncStage::ValidatingTipsets;
                    tasks.push(SyncTask::ValidateTipset {
                        tipset: first_ts.clone(),
                        is_proposed_head: chain.len() == 1,
                    });
                }

                let fork_info = ForkSyncInfo {
                    target_tipset_key: last_ts.key().clone(),
                    target_epoch: last_ts.epoch(),
                    target_sync_epoch_start: first_ts.epoch(),
                    stage,
                    validated_chain_head_epoch: current_validated_epoch,
                    start_time,
                    last_updated: Some(now),
                };

                active_sync_info.push(fork_info);
            }
        }
        (tasks, active_sync_info)
    }
}

#[derive(PartialEq, Eq, Hash, Clone, Debug)]
enum SyncTask {
    ValidateTipset {
        tipset: FullTipset,
        is_proposed_head: bool,
    },
    FetchTipset(TipsetKey, ChainEpoch),
}

impl std::fmt::Display for SyncTask {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SyncTask::ValidateTipset {
                tipset,
                is_proposed_head,
            } => write!(
                f,
                "ValidateTipset(epoch: {}, is_proposed_head: {is_proposed_head})",
                tipset.epoch()
            ),
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
        stateless_mode: bool,
        bad_block_cache: Option<Arc<BadBlockCache>>,
    ) -> Option<SyncEvent> {
        tracing::trace!("SyncTask::execute {self}");
        match self {
            SyncTask::ValidateTipset {
                tipset,
                is_proposed_head,
            } if stateless_mode => Some(SyncEvent::ValidatedTipset {
                tipset,
                is_proposed_head,
            }),
            SyncTask::ValidateTipset {
                tipset,
                is_proposed_head,
            } => match validate_tipset(&state_manager, tipset.clone(), bad_block_cache).await {
                Ok(()) => Some(SyncEvent::ValidatedTipset {
                    tipset,
                    is_proposed_head,
                }),
                Err(e) => {
                    warn!("Error validating tipset: {e}");
                    Some(SyncEvent::BadTipset(tipset))
                }
            },
            SyncTask::FetchTipset(key, epoch) => {
                match get_full_tipset_batch(&network, state_manager.chain_store(), None, &key).await
                {
                    Ok(parents) => Some(SyncEvent::NewFullTipsets(parents)),
                    Err(e) => {
                        tracing::warn!(%key, %epoch, "failed to fetch tipset: {e}");
                        None
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blocks::{Chain4U, HeaderBuilder, chain4u};
    use crate::db::MemoryDB;
    use crate::utils::db::CborStoreExt as _;
    use fil_actors_shared::fvm_ipld_amt::Amtv0 as Amt;
    use num_bigint::BigInt;
    use num_traits::ToPrimitive;
    use std::sync::Arc;
    use tracing::level_filters::LevelFilter;
    use tracing_subscriber::EnvFilter;

    fn setup() -> (Arc<ChainStore<MemoryDB>>, Chain4U<Arc<MemoryDB>>) {
        // Initialize test logger
        let _ = tracing_subscriber::fmt()
            .without_time()
            .with_env_filter(
                EnvFilter::builder()
                    .with_default_directive(LevelFilter::DEBUG.into())
                    .from_env()
                    .unwrap(),
            )
            .try_init();

        let db = Arc::new(MemoryDB::default());

        // Populate DB with message roots used by chain4u
        {
            let empty_amt = Amt::<Cid, _>::new(&db).flush().unwrap();
            db.put_cbor_default(&crate::blocks::TxMeta {
                bls_message_root: empty_amt,
                secp_message_root: empty_amt,
            })
            .unwrap();
        }

        // Create a chain of 5 tipsets using Chain4U
        let c4u = Chain4U::with_blockstore(db.clone());
        chain4u! {
            in c4u;
            [genesis_header = dummy_node(&db, 0)]
        };

        let cs = Arc::new(
            ChainStore::new(
                db.clone(),
                db.clone(),
                db.clone(),
                Default::default(),
                genesis_header.clone().into(),
            )
            .unwrap(),
        );

        cs.set_heaviest_tipset(cs.genesis_tipset()).unwrap();

        (cs, c4u)
    }

    fn dummy_state(db: impl Blockstore, i: ChainEpoch) -> Cid {
        db.put_cbor_default(&i).unwrap()
    }

    fn dummy_node(db: impl Blockstore, i: ChainEpoch) -> HeaderBuilder {
        HeaderBuilder {
            state_root: dummy_state(db, i).into(),
            weight: BigInt::from(i).into(),
            epoch: i.into(),
            ..Default::default()
        }
    }

    #[test]
    fn test_state_machine_validation_order() {
        let (cs, c4u) = setup();
        let db = cs.blockstore().clone();

        chain4u! {
            from [genesis_header] in c4u;
            [a = dummy_node(&db, 1)] -> [b = dummy_node(&db, 2)] -> [c = dummy_node(&db, 3)] -> [d = dummy_node(&db, 4)] -> [e = dummy_node(&db, 5)]
        };

        // Create the state machine
        let mut state_machine = SyncStateMachine::new(cs, Default::default(), true);

        // Insert tipsets in random order
        let tipsets = vec![e, b, d, c, a];

        // Convert each block into a FullTipset and add it to the state machine
        for block in tipsets {
            let full_tipset = FullTipset::new(vec![Block {
                header: block.clone().into(),
                bls_messages: vec![],
                secp_messages: vec![],
            }])
            .unwrap();
            state_machine.update(SyncEvent::NewFullTipsets(vec![full_tipset]));
        }

        // Record validation order by processing all validation tasks in each iteration
        let mut validation_tasks = Vec::new();
        loop {
            let (tasks, _) = state_machine.tasks();

            // Find all validation tasks
            let validation_tipsets: Vec<_> = tasks
                .into_iter()
                .filter_map(|task| {
                    if let SyncTask::ValidateTipset {
                        tipset,
                        is_proposed_head,
                    } = task
                    {
                        Some((tipset, is_proposed_head))
                    } else {
                        None
                    }
                })
                .collect();

            if validation_tipsets.is_empty() {
                break;
            }

            // Record and mark all tipsets as validated
            for (ts, is_proposed_head) in validation_tipsets {
                validation_tasks.push(ts.epoch());
                db.put_cbor_default(&ts.epoch()).unwrap();
                state_machine.mark_validated_tipset(ts, is_proposed_head);
            }
        }

        // We expect validation tasks for epochs 1 through 5 in order
        assert_eq!(validation_tasks, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn test_sync_state_machine_chain_fragments() {
        let (cs, c4u) = setup();
        let db = cs.blockstore().clone();

        // Create a forked chain
        // genesis -> a -> b
        //            \--> c
        chain4u! {
            in c4u;
            [a = dummy_node(&db, 1)] -> [b = dummy_node(&db, 2)]
        };
        chain4u! {
            from [a] in c4u;
            [c = dummy_node(&db, 3)]
        };

        // Create the state machine
        let mut state_machine = SyncStateMachine::new(cs, Default::default(), false);

        // Convert each block into a FullTipset and add it to the state machine
        for block in [a, b, c] {
            let full_tipset = FullTipset::new(vec![Block {
                header: block.clone().into(),
                bls_messages: vec![],
                secp_messages: vec![],
            }])
            .unwrap();
            state_machine.update(SyncEvent::NewFullTipsets(vec![full_tipset]));
        }

        let chains = state_machine
            .chains()
            .into_iter()
            .map(|v| {
                v.into_iter()
                    .map(|ts| ts.weight().to_i64().unwrap_or(0))
                    .collect_vec()
            })
            .collect_vec();

        // Both chains should start at the same tipset
        assert_eq!(chains, vec![vec![1, 3], vec![1, 2]]);
    }
}
