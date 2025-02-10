use std::{ops::Deref as _, sync::Arc};

use ahash::HashSet;
use cid::Cid;
use fvm_ipld_blockstore::Blockstore;
use itertools::Itertools;
use libp2p::PeerId;
use parking_lot::Mutex;
use std::collections::HashMap;
use tokio::task::JoinSet;
use tracing::{debug, info, warn};

use crate::{
    blocks::{Block, FullTipset, TipsetKey},
    chain::ChainStore,
    chain_sync::{metrics, TipsetValidator},
    libp2p::{NetworkEvent, PubsubMessage},
    networks::ChainConfig,
    shim::clock::SECONDS_IN_DAY,
};

use super::network_context::SyncNetworkContext;

// We receive new full tipsets from the p2p swarm, and from miners that use Forest as their frontend.
pub async fn chain_follower<DB: Blockstore + Sync + Send + 'static>(
    chain_config: Arc<ChainConfig>,
    cs: Arc<ChainStore<DB>>,
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
                        if let Ok(tipset) =
                            get_full_tipset(network.clone(), cs.clone(), Some(source), tipset_keys)
                                .await
                                .inspect_err(|e| debug!("Querying full tipset failed: {}", e))
                        {
                            get_full_tipset(
                                network.clone(),
                                cs.clone(),
                                None,
                                tipset.parents().clone(),
                            )
                            .await
                        } else {
                            continue;
                        }
                    }
                    NetworkEvent::PubsubMessage { message } => match message {
                        PubsubMessage::Block(b) => {
                            get_full_tipset(
                                network.clone(),
                                cs.clone(),
                                None,
                                b.header.parents.clone(),
                            )
                            .await
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
                event_sender.send(SyncEvent::NewFullTipsets(vec![Arc::new(tipset)]));
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
                event_sender.send(SyncEvent::NewFullTipsets(vec![tipset]));
            }
            // tipset_receiver is closed, shutdown gracefully
        }
    });

    set.spawn(async move {
        while let Ok(event) = event_receiver.recv_async().await {
            info!("Received event from event receiver.");
            let mut sm = state_machine.lock();
            sm.update(event);
            let mut tasks_set = tasks.lock();
            for task in sm.tasks() {
                // insert task into tasks. If task is already in tasks, skip. If it is not, spawn it.
                let new = tasks_set.insert(task.clone());
                if new {
                    let tasks_clone = tasks.clone();
                    let action =
                        task.clone()
                            .execute(network.clone(), cs.clone(), event_sender.clone());
                    tokio::spawn(async move {
                        action.await;
                        tasks_clone.lock().remove(&task);
                    });
                }
            }
            info!("Current number of tasks: {}", tasks_set.len());
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
    network
        .chain_exchange_fts(peer_id, &tipset_keys.clone())
        .await
        .map_err(|e| anyhow::anyhow!(e))
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

        if self.is_validated(&tipset) {
            info!("Add tipset: Already validated. epoch: {:?}", tipset.epoch());
            return;
        }

        // Check if tipset already exists
        if self.tipsets.contains_key(tipset.key()) {
            info!("Add tipset: Already in map. epoch: {:?}", tipset.epoch());
            return;
        }

        info!("Add tipset: Adding to map. epoch: {:?}", tipset.epoch());
        self.tipsets.insert(tipset.key().clone(), tipset);
    }

    fn mark_bad_block(&mut self, cid: Cid) {
        todo!()
    }

    fn mark_validated_tipset(&mut self, tipset: Arc<FullTipset>) {
        todo!()
    }

    pub fn update(&mut self, event: SyncEvent) {
        let heaviest = self.cs.heaviest_tipset();
        info!(
            "Sync StateMachine: Current heaviest tipset epoch: {}",
            heaviest.epoch()
        );

        match event {
            SyncEvent::NewFullTipsets(tipsets) => {
                for tipset in tipsets {
                    self.add_full_tipset(tipset);
                }
            }
            SyncEvent::BadBlock(cid) => self.mark_bad_block(cid),
            SyncEvent::ValidatedTipset(tipset) => self.mark_validated_tipset(tipset),
        }
        for (i, chain) in self.chains().iter().enumerate() {
            if let (Some(first), Some(last)) = (chain.first(), chain.last()) {
                info!(
                    "Sync StateMachine: Chain: {}: {} ({}) .. {} ({})",
                    i,
                    first.epoch(),
                    first.blocks().len(),
                    last.epoch(),
                    last.blocks().len()
                );
            }
        }
    }

    pub fn tasks(&self) -> Vec<SyncTask> {
        let mut tasks = Vec::new();
        for chain in self.chains() {
            if let Some(first_ts) = chain.first() {
                if !self.is_ready_for_validation(first_ts) {
                    tasks.push(SyncTask::FetchTipset(first_ts.parents().clone()));
                } else {
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
        sender: flume::Sender<SyncEvent>,
    ) {
        match self {
            SyncTask::ValidateTipset(tipset) => {
                // cs.validate_tipset(tipset).await;
            }
            SyncTask::FetchTipset(key) => {
                info!("Fetching tipset: {}", key);
                if let Ok(parent) = get_full_tipset(network.clone(), cs.clone(), None, key).await {
                    sender.send(SyncEvent::NewFullTipsets(vec![Arc::new(parent)]));
                }
            }
        }
    }
}
