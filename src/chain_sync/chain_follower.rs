use std::{ops::Deref as _, sync::Arc};

use crate::state_manager::StateManager;
use ahash::HashSet;
use cid::Cid;
use fvm_ipld_blockstore::Blockstore;
use itertools::Itertools;
use libp2p::PeerId;
use parking_lot::Mutex;
use std::collections::HashMap;
use tokio::task::JoinSet;
use tracing::{debug, info, warn};

use crate::chain_sync::tipset_syncer::validate_tipset;
use crate::chain_sync::tipset_syncer::InvalidBlockStrategy;
use crate::{
    blocks::{Block, FullTipset, TipsetKey},
    chain::ChainStore,
    chain_sync::{bad_block_cache::BadBlockCache, metrics, TipsetValidator},
    libp2p::{NetworkEvent, PubsubMessage},
    networks::ChainConfig,
    shim::clock::SECONDS_IN_DAY,
};

use super::network_context::SyncNetworkContext;

// We receive new full tipsets from the p2p swarm, and from miners that use Forest as their frontend.
pub async fn chain_follower<DB: Blockstore + Sync + Send + 'static>(
    chain_config: Arc<ChainConfig>,
    cs: Arc<ChainStore<DB>>,
    state_manager: Arc<StateManager<DB>>,
    bad_block_cache: Arc<BadBlockCache>,
    network_rx: flume::Receiver<NetworkEvent>,
    tipset_receiver: flume::Receiver<Arc<FullTipset>>,
    network: SyncNetworkContext<DB>,
) -> anyhow::Result<()> {
    let state_machine = Arc::new(Mutex::new(SyncStateMachine::new(cs.clone(), chain_config)));
    let tasks: Arc<Mutex<HashSet<SyncTask>>> = Arc::new(Mutex::new(HashSet::default()));

    let (event_sender, event_receiver) = flume::bounded(20);

    let mut set = JoinSet::new();

    set.spawn({
        let event_sender = event_sender.clone();
        let network = network.clone();
        let cs = cs.clone();
        async move {
            while let Ok(event) = network_rx.recv_async().await {
                // inc metrics (TODO)
                // update peer manager (TODO)
                // fetch full tipsets from network (TODO)
                let Ok(tipset) = (match event {
                    NetworkEvent::HelloResponseOutbound { request, source } => {
                        let tipset_keys = TipsetKey::from(request.heaviest_tip_set.clone());
                        get_full_tipset(network.clone(), cs.clone(), Some(source), tipset_keys)
                            .await
                            .inspect_err(|e| debug!("Querying full tipset failed: {}", e))
                    }
                    NetworkEvent::PubsubMessage { message } => match message {
                        PubsubMessage::Block(b) => {
                            let key = TipsetKey::from(nunny::vec![*b.header.cid()]);
                            get_full_tipset(network.clone(), cs.clone(), None, key).await
                        }
                        PubsubMessage::Message(m) => {
                            // handle_pubsub_message(mem_pool, m);
                            continue;
                        }
                    },
                    _ => continue,
                }) else {
                    continue;
                };
                let _ = event_sender
                    .send_async(SyncEvent::NewFullTipsets(vec![Arc::new(tipset)]))
                    .await;
            }
        }
    });

    // spawn a task to tipsets to the state machine. These tipsets are received
    // from the p2p swarm and from directly-connected miners.
    set.spawn({
        let event_sender = event_sender.clone();
        async move {
            while let Ok(tipset) = tipset_receiver.recv_async().await {
                info!("Received tipset from tipset receiver.");
                let _ = event_sender
                    .send_async(SyncEvent::NewFullTipsets(vec![tipset]))
                    .await;
            }
            // tipset_receiver is closed, shutdown gracefully
        }
    });

    set.spawn({
        let bad_block_cache = bad_block_cache.clone();
        async move {
            while let Ok(event) = event_receiver.recv_async().await {
                // info!("Received event from event receiver.");
                let mut sm = state_machine.lock();
                sm.update(event);
                let mut tasks_set = tasks.lock();
                for task in sm.tasks() {
                    // insert task into tasks. If task is already in tasks, skip. If it is not, spawn it.
                    let new = tasks_set.insert(task.clone());
                    if new {
                        let tasks_clone = tasks.clone();
                        let action = task.clone().execute(
                            network.clone(),
                            cs.clone(),
                            state_manager.clone(),
                            bad_block_cache.clone(),
                            event_sender.clone(),
                        );
                        tokio::spawn(async move {
                            action.await;
                            tasks_clone.lock().remove(&task);
                        });
                    }
                }
                // info!("Current number of tasks: {}", tasks_set.len());
            }
        }
    });

    set.join_all().await;
    Ok(())
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
    BadBlock(Cid),
    ValidatedTipset(Arc<FullTipset>),
}

struct SyncStateMachine<DB> {
    chain_config: Arc<ChainConfig>,
    cs: Arc<ChainStore<DB>>,
    // Map from TipsetKey to FullTipset
    tipsets: HashMap<TipsetKey, Arc<FullTipset>>,
}

impl<DB: Blockstore> SyncStateMachine<DB> {
    pub fn new(cs: Arc<ChainStore<DB>>, chain_config: Arc<ChainConfig>) -> Self {
        Self {
            cs,
            chain_config,
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
            None,
            &self.cs.genesis_tipset(),
            self.chain_config.block_delay_secs,
        ) {
            metrics::INVALID_TIPSET_TOTAL.inc();
            warn!("Skipping invalid tipset: {}", why);
            return;
        }

        // Check if tipset is older than a day compared to heaviest tipset
        let heaviest = self.cs.heaviest_tipset();
        let epoch_diff = heaviest.epoch() - tipset.epoch();
        let time_diff = epoch_diff * (self.chain_config.block_delay_secs as i64);

        if time_diff > SECONDS_IN_DAY {
            info!(
                "Add tipset: Ignoring old tipset. epoch: {}, heaviest: {}, diff: {}s",
                tipset.epoch(),
                heaviest.epoch(),
                time_diff
            );
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

        // info!("Add tipset: Adding to map. epoch: {:?}", tipset.epoch());

        // Find any existing tipsets with same epoch and parents
        let mut to_remove = Vec::new();
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
        if let Ok(merged_tipset) = FullTipset::new(merged_blocks.into_iter()) {
            self.tipsets
                .insert(merged_tipset.key().clone(), Arc::new(merged_tipset));
        }
    }

    fn mark_bad_block(&mut self, cid: Cid) {
        todo!()
    }

    fn mark_validated_tipset(&mut self, tipset: Arc<FullTipset>) {
        // FIXME: Should navigate to the heaviest tipset in the chain
        assert!(self.is_validated(&tipset), "Tipset must be validated");
        self.tipsets.remove(tipset.key());
        let tipset = Arc::new(tipset.deref().clone().into_tipset());
        self.cs.set_heaviest_tipset(tipset);
    }

    pub fn update(&mut self, event: SyncEvent) {
        let heaviest = self.cs.heaviest_tipset();

        let prev_count = self.tipsets.len();

        match event {
            SyncEvent::NewFullTipsets(tipsets) => {
                for tipset in tipsets {
                    self.add_full_tipset(tipset);
                }
            }
            SyncEvent::BadBlock(cid) => self.mark_bad_block(cid),
            SyncEvent::ValidatedTipset(tipset) => self.mark_validated_tipset(tipset),
        }
        if prev_count != self.tipsets.len() {
            info!(
                "Sync StateMachine: Current heaviest tipset epoch: {}",
                heaviest.epoch()
            );
            for (i, chain) in self.chains().iter().enumerate() {
                if let (Some(first), Some(last)) = (chain.first(), chain.last()) {
                    info!(
                        "Sync StateMachine: Chain: {}: {} ({}) .. {} ({}) [length: {}]",
                        i,
                        first.epoch(),
                        first.blocks().len(),
                        last.epoch(),
                        last.blocks().len(),
                        last.epoch() - first.epoch()
                    );
                }
            }
        }
    }

    pub fn tasks(&self) -> Vec<SyncTask> {
        let mut tasks = Vec::new();
        for chain in self.chains() {
            if let Some(first_ts) = chain.first() {
                if !self.is_ready_for_validation(first_ts) {
                    // FIXME: This is just here to ignore forks.
                    if first_ts.epoch() > self.cs.heaviest_tipset().epoch() {
                        tasks.push(SyncTask::FetchTipset(first_ts.parents().clone()));
                    }
                } else {
                    // info!("Epoch {} ready for validation", first_ts.epoch());
                    tasks.push(SyncTask::ValidateTipset(first_ts.clone()));
                }
            }
        }
        tasks
    }
}

#[derive(PartialEq, Eq, Hash, Clone)]
enum SyncTask {
    ValidateTipset(Arc<FullTipset>),
    FetchTipset(TipsetKey),
}

impl SyncTask {
    async fn execute<DB: Blockstore + Sync + Send + 'static>(
        self,
        network: SyncNetworkContext<DB>,
        cs: Arc<ChainStore<DB>>,
        state_manager: Arc<StateManager<DB>>,
        bad_block_cache: Arc<BadBlockCache>,
        sender: flume::Sender<SyncEvent>,
    ) {
        match self {
            SyncTask::ValidateTipset(tipset) => {
                info!("Validating tipset at epoch {}", tipset.epoch());
                // cs.validate_tipset(tipset).await;
                let genesis = cs.genesis_tipset();
                match validate_tipset(
                    state_manager,
                    &cs,
                    &bad_block_cache,
                    tipset.deref().clone(),
                    &genesis,
                    InvalidBlockStrategy::Forgiving,
                )
                .await
                {
                    Ok(()) => {
                        sender.send_async(SyncEvent::ValidatedTipset(tipset)).await;
                    }
                    Err(e) => {
                        warn!("Error validating tipset: {}", e);
                        sender
                            .send_async(SyncEvent::BadBlock(tipset.blocks()[0].cid().clone()))
                            .await;
                    }
                }
            }
            SyncTask::FetchTipset(key) => {
                // info!("Fetching tipset: {}", key);
                if let Ok(parent) = get_full_tipset(network.clone(), cs.clone(), None, key).await {
                    sender
                        .send_async(SyncEvent::NewFullTipsets(vec![Arc::new(parent)]))
                        .await;
                }
            }
        }
    }
}
