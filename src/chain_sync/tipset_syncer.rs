// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{
    cmp::{min, Ordering},
    convert::TryFrom,
    future::Future,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::blocks::{
    Block, BlockHeader, Error as ForestBlockError, FullTipset, Tipset, TipsetKeys,
};
use crate::chain::{persist_objects, ChainStore, Error as ChainStoreError};
use crate::libp2p::chain_exchange::TipsetBundle;
use crate::message::{valid_for_block_inclusion, Message as MessageTrait};
use crate::networks::Height;
use crate::shim::{
    address::Address, clock::ChainEpoch, crypto::verify_bls_aggregate, econ::BLOCK_GAS_LIMIT,
    gas::price_list_by_network_version, message::Message, state_tree::StateTree,
};
use crate::state_manager::{is_valid_for_sending, Error as StateManagerError, StateManager};
use crate::utils::io::ProgressBar;
use ahash::{HashMap, HashMapExt, HashSet};
use cid::Cid;
use futures::{stream::FuturesUnordered, Stream, StreamExt, TryFutureExt};
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::to_vec;
use fvm_shared::ALLOWABLE_CLOCK_DRIFT;
use log::{debug, error, info, trace, warn};
use nonempty::NonEmpty;
use num::BigInt;
use thiserror::Error;

use crate::chain_sync::{
    bad_block_cache::BadBlockCache,
    consensus::{collect_errs, Consensus},
    metrics,
    network_context::SyncNetworkContext,
    sync_state::SyncStage,
    validation::TipsetValidator,
};

const MAX_TIPSETS_TO_REQUEST: u64 = 100;

#[derive(Debug, Error)]
pub enum TipsetProcessorError<C: Consensus> {
    #[error("TipsetRangeSyncer error: {0}")]
    RangeSyncer(#[from] TipsetRangeSyncerError<C>),
    #[error("Tipset stream closed")]
    StreamClosed,
    #[error("Tipset has already been synced")]
    AlreadySynced,
}

#[derive(Debug, Error)]
pub enum TipsetRangeSyncerError<C: Consensus> {
    #[error("Tipset range length is less than 0")]
    InvalidTipsetRangeLength,
    #[error("Provided tiset does not match epoch for the range")]
    InvalidTipsetEpoch,
    #[error("Provided tipset does not match parent for the range")]
    InvalidTipsetParent,
    #[error("Block must have a signature")]
    BlockWithoutSignature,
    #[error("Block without BLS aggregate signature")]
    BlockWithoutBlsAggregate,
    #[error("Block received from the future: now = {0}, block = {1}")]
    TimeTravellingBlock(u64, u64),
    #[error("Tipset range contains bad block [block = {0}]: {1}")]
    TipsetRangeWithBadBlock(Cid, String),
    #[error("Validation error: {0}")]
    Validation(String),
    #[error("Processing error: {0}")]
    Calculation(String),
    #[error("Chain store error: {0}")]
    ChainStore(#[from] ChainStoreError),
    #[error("StateManager error: {0}")]
    StateManager(#[from] StateManagerError),
    #[error("Block error: {0}")]
    BlockError(#[from] ForestBlockError),
    #[error("Chain fork length exceeds the maximum")]
    ChainForkLengthExceedsMaximum,
    #[error("Chain fork length exceeds finality threshold")]
    ChainForkLengthExceedsFinalityThreshold,
    #[error("Chain for block forked from local chain at genesis, refusing to sync block: {0}")]
    ForkAtGenesisBlock(String),
    #[error("Querying tipsets from the network failed: {0}")]
    NetworkTipsetQueryFailed(String),
    #[error("Query tipset messages from the network failed: {0}")]
    NetworkMessageQueryFailed(String),
    #[error("BLS aggregate signature {0} was invalid for msgs {1}")]
    BlsAggregateSignatureInvalid(String, String),
    #[error("Message signature invalid: {0}")]
    MessageSignatureInvalid(String),
    #[error("Block message root does not match: expected {0}, computed {1}")]
    BlockMessageRootInvalid(String, String),
    #[error("Computing message root failed: {0}")]
    ComputingMessageRoot(String),
    #[error("Resolving address from message failed: {0}")]
    ResolvingAddressFromMessage(String),
    #[error("Generating Tipset from bundle failed: {0}")]
    GeneratingTipsetFromTipsetBundle(String),
    #[error("Loading tipset parent from the store failed: {0}")]
    TipsetParentNotFound(ChainStoreError),
    #[error("Consensus error: {0}")]
    ConsensusError(C::Error),
}

impl<C: Consensus, T> From<flume::SendError<T>> for TipsetRangeSyncerError<C> {
    fn from(err: flume::SendError<T>) -> Self {
        TipsetRangeSyncerError::NetworkTipsetQueryFailed(format!("{err}"))
    }
}

impl<C: Consensus> From<tokio::task::JoinError> for TipsetRangeSyncerError<C> {
    fn from(err: tokio::task::JoinError) -> Self {
        TipsetRangeSyncerError::NetworkTipsetQueryFailed(format!("{err}"))
    }
}

impl<C: Consensus> TipsetRangeSyncerError<C> {
    /// Concatenate all validation error messages into one comma separated
    /// version.
    fn concat(errs: NonEmpty<TipsetRangeSyncerError<C>>) -> Self {
        let msg = errs
            .iter()
            .map(|e| e.to_string())
            .collect::<Vec<_>>()
            .join(", ");

        TipsetRangeSyncerError::Validation(msg)
    }
}

struct TipsetGroup {
    tipsets: Vec<Arc<Tipset>>,
    epoch: ChainEpoch,
    parents: TipsetKeys,
}

impl TipsetGroup {
    fn new(tipset: Arc<Tipset>) -> Self {
        let epoch = tipset.epoch();
        let parents = tipset.parents().clone();
        Self {
            tipsets: vec![tipset],
            epoch,
            parents,
        }
    }

    fn epoch(&self) -> ChainEpoch {
        self.epoch
    }

    fn parents(&self) -> TipsetKeys {
        self.parents.clone()
    }
    // Attempts to add a tipset to the group
    // If the tipset is added, the method returns `None`
    // If the tipset is discarded, the method return `Some(tipset)`
    fn try_add_tipset(&mut self, tipset: Arc<Tipset>) -> Option<Arc<Tipset>> {
        // The new tipset must:
        //  1. Be unique
        //  2. Have the same epoch and parents as the other tipsets in the group
        if !self.epoch.eq(&tipset.epoch()) || !self.parents.eq(tipset.parents()) {
            return Some(tipset);
        }
        if self.tipsets.iter().any(|ts| tipset.key().eq(ts.key())) {
            return Some(tipset);
        }
        self.tipsets.push(tipset);
        None
    }

    fn take_heaviest_tipset(&mut self) -> Option<Arc<Tipset>> {
        let (index, _) = self.heaviest_weight();
        Some(self.tipsets.swap_remove(index))
    }

    fn heaviest_weight(&self) -> (usize, &BigInt) {
        // Unwrapping is safe because we initialize the struct with at least one tipset
        let max = self.tipsets.iter().map(|ts| ts.weight()).max().unwrap();

        let ties = self
            .tipsets
            .iter()
            .enumerate()
            .filter(|(_, ts)| ts.weight() == max);

        let (index, ts) = ties
            .reduce(|(i, ts), (j, other)| {
                // break the tie
                if ts.break_weight_tie(other) {
                    (i, ts)
                } else {
                    (j, other)
                }
            })
            .unwrap();
        (index, ts.weight())
    }

    fn merge(&mut self, other: Self) {
        other.tipsets.into_iter().for_each(|ts| {
            self.try_add_tipset(ts);
        });
    }

    fn is_mergeable(&self, other: &Self) -> bool {
        self.epoch.eq(&other.epoch) && self.parents.eq(&other.parents)
    }

    fn is_heavier_than(&self, other: &Self) -> bool {
        self.weight_cmp(other).is_gt()
    }

    fn weight_cmp(&self, other: &Self) -> Ordering {
        let (i, weight) = self.heaviest_weight();
        let (j, otherw) = other.heaviest_weight();
        match weight.cmp(otherw) {
            Ordering::Equal => {
                if self.tipsets[i].break_weight_tie(&other.tipsets[j]) {
                    Ordering::Greater
                } else {
                    Ordering::Equal
                }
            }
            r => r,
        }
    }

    fn tipsets(self) -> Vec<Arc<Tipset>> {
        self.tipsets
    }
}

/// The `TipsetProcessor` receives and prioritizes a stream of Tipsets
/// for syncing from the `ChainMuxer` and the `SyncSubmitBlock` API before
/// syncing. Each unique Tipset, by epoch and parents, is mapped into a Tipset
/// range which will be synced into the Chain Store.
pub(in crate::chain_sync) struct TipsetProcessor<DB, C: Consensus> {
    state: TipsetProcessorState<DB, C>,
    tracker: crate::chain_sync::chain_muxer::WorkerState,
    /// Tipsets pushed into this stream _must_ be validated beforehand by the
    /// `TipsetValidator`
    tipsets: Pin<Box<dyn futures::Stream<Item = Arc<Tipset>> + Send>>,
    consensus: Arc<C>,
    state_manager: Arc<StateManager<DB>>,
    network: SyncNetworkContext<DB>,
    chain_store: Arc<ChainStore<DB>>,
    bad_block_cache: Arc<BadBlockCache>,
    genesis: Arc<Tipset>,
}

impl<DB, C> TipsetProcessor<DB, C>
where
    DB: Blockstore + Clone + Sync + Send + 'static,
    C: Consensus,
{
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        tracker: crate::chain_sync::chain_muxer::WorkerState,
        tipsets: Pin<Box<dyn futures::Stream<Item = Arc<Tipset>> + Send>>,
        consensus: Arc<C>,
        state_manager: Arc<StateManager<DB>>,
        network: SyncNetworkContext<DB>,
        chain_store: Arc<ChainStore<DB>>,
        bad_block_cache: Arc<BadBlockCache>,
        genesis: Arc<Tipset>,
    ) -> Self {
        Self {
            state: TipsetProcessorState::Idle,
            tracker,
            tipsets,
            consensus,
            state_manager,
            network,
            chain_store,
            bad_block_cache,
            genesis,
        }
    }

    fn find_range(
        &self,
        mut tipset_group: TipsetGroup,
    ) -> TipsetProcessorFuture<TipsetRangeSyncer<DB, C>, TipsetProcessorError<C>> {
        let consensus = self.consensus.clone();
        let state_manager = self.state_manager.clone();
        let chain_store = self.chain_store.clone();
        let network = self.network.clone();
        let bad_block_cache = self.bad_block_cache.clone();
        let tracker = self.tracker.clone();
        let genesis = self.genesis.clone();
        Box::pin(async move {
            // Define the low end of the range
            // Unwrapping is safe here because the store always has at least one tipset
            let current_head = chain_store.heaviest_tipset();
            // Unwrapping is safe here because we assume that the
            // tipset group contains at least one tipset
            let proposed_head = tipset_group.take_heaviest_tipset().unwrap();

            if current_head.key().eq(proposed_head.key()) {
                return Err(TipsetProcessorError::AlreadySynced);
            }

            let mut tipset_range_syncer = TipsetRangeSyncer::new(
                tracker,
                proposed_head,
                current_head,
                consensus,
                state_manager,
                network,
                chain_store,
                bad_block_cache,
                genesis,
            )?;
            for tipset in tipset_group.tipsets() {
                tipset_range_syncer.add_tipset(tipset)?;
            }
            Ok(tipset_range_syncer)
        })
    }
}

type TipsetProcessorFuture<T, E> = Pin<Box<dyn Future<Output = Result<T, E>> + Send>>;

enum TipsetProcessorState<DB, C: Consensus> {
    Idle,
    FindRange {
        range_finder: TipsetProcessorFuture<TipsetRangeSyncer<DB, C>, TipsetProcessorError<C>>,
        epoch: i64,
        parents: TipsetKeys,
        current_sync: Option<TipsetGroup>,
        next_sync: Option<TipsetGroup>,
    },
    SyncRange {
        range_syncer: Pin<Box<TipsetRangeSyncer<DB, C>>>,
        next_sync: Option<TipsetGroup>,
    },
}

impl<DB, C> Future for TipsetProcessor<DB, C>
where
    DB: Blockstore + Clone + Sync + Send + 'static,
    C: Consensus,
{
    type Output = Result<(), TipsetProcessorError<C>>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        trace!("Polling TipsetProcessor");

        // TODO: Determine if polling the tipset stream before the state machine
        //       introduces a DOS attack vector where peers send duplicate, valid
        // tipsets over       GossipSub to divert resources away from syncing
        // tipset ranges. First, gather the tipsets off of the channel. Reading
        // off the receiver will return immediately. Ensure that the task will
        // wake up when the stream has a new item by registering it for wakeup.
        // As a tipset is received through the stream we assume:
        //   1. Tipset has at least 1 block
        //   2. Tipset epoch is not behind the current max epoch in the store
        //   3. Tipset is heavier than the heaviest tipset in the store at the time when
        // it was queued   4. Tipset message roots were calculated and integrity
        // checks were run

        // Read all of the tipsets available on the stream
        let mut grouped_tipsets: HashMap<(i64, TipsetKeys), TipsetGroup> = HashMap::new();
        loop {
            match self.tipsets.as_mut().poll_next(cx) {
                Poll::Ready(Some(tipset)) => {
                    let key = (tipset.epoch(), tipset.parents().clone());
                    match grouped_tipsets.get_mut(&key) {
                        None => {
                            grouped_tipsets.insert(key, TipsetGroup::new(tipset));
                        }
                        Some(group) => {
                            group.try_add_tipset(tipset);
                        }
                    }
                }
                Poll::Ready(None) => {
                    // Stream should never close
                    return Poll::Ready(Err(TipsetProcessorError::StreamClosed));
                }
                // The current task is registered for wakeup when this
                // stream has a new item available. Break here to make
                // forward progress on syncing before polling the stream again.
                Poll::Pending => break,
            }
        }

        trace!("Tipsets received through stream: {}", grouped_tipsets.len());

        // Consume the tipsets read off of the stream and attempt to update the state
        // machine
        match self.state {
            TipsetProcessorState::Idle => {
                // Set the state to FindRange if we have a tipset to sync towards
                // Consume the tipsets received, start syncing the heaviest tipset group, and
                // discard the rest
                if let Some(((epoch, parents), heaviest_tipset_group)) = grouped_tipsets
                    .into_iter()
                    .max_by(|(_, a), (_, b)| a.weight_cmp(b))
                {
                    trace!("Finding range for tipset epoch = {}", epoch);
                    self.state = TipsetProcessorState::FindRange {
                        epoch,
                        parents,
                        range_finder: self.find_range(heaviest_tipset_group),
                        current_sync: None,
                        next_sync: None,
                    };
                }
            }
            TipsetProcessorState::FindRange {
                ref mut epoch,
                ref mut parents,
                ref mut current_sync,
                ref mut next_sync,
                ..
            } => {
                // Add tipsets to the current sync cache
                if let Some(tipset_group) = grouped_tipsets.remove(&(*epoch, parents.clone())) {
                    match current_sync {
                        Some(cs) => {
                            // This check is redundant
                            if cs.is_mergeable(&tipset_group) {
                                cs.merge(tipset_group);
                            }
                        }
                        None => *current_sync = Some(tipset_group),
                    }
                }

                // Update or replace the next sync
                if let Some(heaviest_tipset_group) = grouped_tipsets
                    .into_iter()
                    .max_by(|(_, a), (_, b)| a.weight_cmp(b))
                    .map(|(_, group)| group)
                {
                    // Find the heaviest tipset group and either merge it with the
                    // tipset group in the next_sync or replace it.
                    match next_sync {
                        None => *next_sync = Some(heaviest_tipset_group),
                        Some(ns) => {
                            if ns.is_mergeable(&heaviest_tipset_group) {
                                // Both tipsets groups have the same epoch & parents, so merge them
                                ns.merge(heaviest_tipset_group);
                            } else if heaviest_tipset_group.is_heavier_than(ns) {
                                // The tipset group received is heavier than the one saved, replace
                                // it.
                                *next_sync = Some(heaviest_tipset_group);
                            }
                            // Otherwise, drop the heaviest tipset group
                        }
                    }
                }
            }
            TipsetProcessorState::SyncRange {
                ref mut range_syncer,
                ref mut next_sync,
            } => {
                // Add tipsets to the current tipset range syncer
                if let Some(tipset_group) = grouped_tipsets.remove(&(
                    range_syncer.proposed_head_epoch(),
                    range_syncer.proposed_head_parents(),
                )) {
                    tipset_group.tipsets().into_iter().for_each(|ts| {
                        let tipset_key = ts.key().clone();
                        match range_syncer.add_tipset(ts) {
                            Ok(added) => {
                                if added {
                                    trace!("Successfully added tipset [key = {:?}] to running range syncer", tipset_key);
                                }
                            }
                            Err(why) => {
                                error!("Adding tipset to range syncer failed: {}", why);
                            }
                        }
                    });
                }

                // Update or replace the next sync
                if let Some(heaviest_tipset_group) = grouped_tipsets
                    .into_iter()
                    .max_by(|(_, a), (_, b)| a.weight_cmp(b))
                    .map(|(_, group)| group)
                {
                    // Find the heaviest tipset group and either merge it with the
                    // tipset group in the next_sync or replace it.
                    match next_sync {
                        None => *next_sync = Some(heaviest_tipset_group),
                        Some(ns) => {
                            if ns.is_mergeable(&heaviest_tipset_group) {
                                // Both tipsets groups have the same epoch & parents, so merge them
                                ns.merge(heaviest_tipset_group);
                            } else if heaviest_tipset_group.is_heavier_than(ns) {
                                // The tipset group received is heavier than the one saved, replace
                                // it.
                                *next_sync = Some(heaviest_tipset_group);
                            } else {
                                // Otherwise, drop the heaviest tipset group
                                trace!("Dropping collected tipset groups");
                            }
                        }
                    }
                }
            }
        }

        // Drive underlying futures to completion
        loop {
            match self.state {
                TipsetProcessorState::Idle => {
                    // Tipsets stream must have already registered with the task driver
                    // to wake up this task when a new tipset is available.
                    // Otherwise, the future can stall.
                    return Poll::Pending;
                }
                TipsetProcessorState::FindRange {
                    ref mut range_finder,
                    ref mut current_sync,
                    ref mut next_sync,
                    ..
                } => match range_finder.as_mut().poll(cx) {
                    Poll::Ready(Ok(mut range_syncer)) => {
                        debug!(
                            "Determined epoch range for next sync: [{}, {}]",
                            range_syncer.current_head.epoch(),
                            range_syncer.proposed_head.epoch(),
                        );
                        // Add current_sync to the yielded range syncer.
                        // These tipsets match the range's [epoch, parents]
                        if let Some(tipset_group) = current_sync.take() {
                            tipset_group.tipsets().into_iter().for_each(|ts| {
                                if let Err(why) = range_syncer.add_tipset(ts) {
                                    error!("Adding tipset to range syncer failed: {}", why);
                                }
                            });
                        }
                        self.state = TipsetProcessorState::SyncRange {
                            range_syncer: Box::pin(range_syncer),
                            next_sync: next_sync.take(),
                        };
                    }
                    Poll::Ready(Err(why)) => {
                        match why {
                            // Do not log for these errors since they are expected to occur
                            // throughout the syncing process
                            TipsetProcessorError::AlreadySynced
                            | TipsetProcessorError::RangeSyncer(
                                TipsetRangeSyncerError::InvalidTipsetRangeLength,
                            ) => (),
                            why => {
                                error!("Finding tipset range for sync failed: {}", why);
                            }
                        };
                        self.state = TipsetProcessorState::Idle;
                    }
                    Poll::Pending => return Poll::Pending,
                },
                TipsetProcessorState::SyncRange {
                    ref mut range_syncer,
                    ref mut next_sync,
                } => {
                    let proposed_head_epoch = range_syncer.proposed_head.epoch();
                    let current_head_epoch = range_syncer.current_head.epoch();
                    // Drive the range_syncer to completion
                    match range_syncer.as_mut().poll(cx) {
                        Poll::Ready(Ok(_)) => {
                            metrics::HEAD_EPOCH.set(proposed_head_epoch as u64);
                            info!(
                                "Successfully synced tipset range: [{}, {}]",
                                current_head_epoch, proposed_head_epoch,
                            );
                        }
                        Poll::Ready(Err(why)) => {
                            metrics::TIPSET_RANGE_SYNC_FAILURE_TOTAL.inc();
                            error!(
                                "Syncing tipset range [{}, {}] failed: {}",
                                current_head_epoch, proposed_head_epoch, why,
                            );
                        }
                        Poll::Pending => return Poll::Pending,
                    }
                    // Move to the next state
                    match next_sync.take() {
                        // This tipset group is the heaviest that has been received while
                        // rnning this tipset range syncer, so start syncing it
                        Some(tipset_group) => {
                            self.state = TipsetProcessorState::FindRange {
                                epoch: tipset_group.epoch(),
                                parents: tipset_group.parents(),
                                range_finder: self.find_range(tipset_group),
                                current_sync: None,
                                next_sync: None,
                            };
                        }
                        None => {
                            self.state = TipsetProcessorState::Idle;
                        }
                    }
                }
            }
        }
    }
}

#[derive(Debug, Copy, Clone)]
enum InvalidBlockStrategy {
    Strict,
    Forgiving,
}

type TipsetRangeSyncerFuture<C> =
    Pin<Box<dyn Future<Output = Result<(), TipsetRangeSyncerError<C>>> + Send>>;

pub(in crate::chain_sync) struct TipsetRangeSyncer<DB, C: Consensus> {
    pub proposed_head: Arc<Tipset>,
    pub current_head: Arc<Tipset>,
    tipsets_included: HashSet<TipsetKeys>,
    tipset_tasks: Pin<Box<FuturesUnordered<TipsetRangeSyncerFuture<C>>>>,
    state_manager: Arc<StateManager<DB>>,
    network: SyncNetworkContext<DB>,
    chain_store: Arc<ChainStore<DB>>,
    bad_block_cache: Arc<BadBlockCache>,
    genesis: Arc<Tipset>,
    consensus: Arc<C>,
}

impl<DB, C> TipsetRangeSyncer<DB, C>
where
    DB: Blockstore + Clone + Sync + Send + 'static,
    C: Consensus,
{
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        tracker: crate::chain_sync::chain_muxer::WorkerState,
        proposed_head: Arc<Tipset>,
        current_head: Arc<Tipset>,
        consensus: Arc<C>,
        state_manager: Arc<StateManager<DB>>,
        network: SyncNetworkContext<DB>,
        chain_store: Arc<ChainStore<DB>>,
        bad_block_cache: Arc<BadBlockCache>,
        genesis: Arc<Tipset>,
    ) -> Result<Self, TipsetRangeSyncerError<C>> {
        let tipset_tasks = Box::pin(FuturesUnordered::new());
        let tipset_range_length = proposed_head.epoch() - current_head.epoch();

        // Ensure the difference in epochs between the proposed and current head is >= 0
        if tipset_range_length < 0 {
            return Err(TipsetRangeSyncerError::InvalidTipsetRangeLength);
        }

        tipset_tasks.push(sync_tipset_range(
            proposed_head.clone(),
            current_head.clone(),
            tracker,
            // Casting from i64 -> u64 is safe because we ensured that
            // the value is greater than 0
            tipset_range_length as u64,
            consensus.clone(),
            state_manager.clone(),
            chain_store.clone(),
            network.clone(),
            bad_block_cache.clone(),
            genesis.clone(),
        ));

        let tipsets_included = HashSet::from_iter([proposed_head.key().clone()]);
        Ok(Self {
            proposed_head,
            current_head,
            tipsets_included,
            tipset_tasks,
            consensus,
            state_manager,
            network,
            chain_store,
            bad_block_cache,
            genesis,
        })
    }

    pub fn add_tipset(
        &mut self,
        additional_head: Arc<Tipset>,
    ) -> Result<bool, TipsetRangeSyncerError<C>> {
        let new_key = additional_head.key().clone();
        // Ignore duplicate tipsets
        if self.tipsets_included.contains(&new_key) {
            return Ok(false);
        }
        // Verify that the proposed Tipset has the same epoch and parent
        // as the original proposed Tipset
        if additional_head.epoch() != self.proposed_head.epoch() {
            error!(
                "Epoch for tipset ({}) added to TipsetSyncer does not match initialized tipset ({})",
                additional_head.epoch(),
                self.proposed_head.epoch(),
            );
            return Err(TipsetRangeSyncerError::InvalidTipsetEpoch);
        }
        if additional_head.parents() != self.proposed_head.parents() {
            error!("Parents for tipset added to TipsetSyncer do not match initialized tipset");
            return Err(TipsetRangeSyncerError::InvalidTipsetParent);
        }
        // Keep track of tipsets included
        self.tipsets_included.insert(new_key);

        self.tipset_tasks.push(sync_tipset(
            additional_head,
            self.consensus.clone(),
            self.state_manager.clone(),
            self.chain_store.clone(),
            self.network.clone(),
            self.bad_block_cache.clone(),
            self.genesis.clone(),
        ));
        Ok(true)
    }

    pub fn proposed_head_epoch(&self) -> i64 {
        self.proposed_head.epoch()
    }

    pub fn proposed_head_parents(&self) -> TipsetKeys {
        self.proposed_head.parents().clone()
    }
}

impl<DB, C> Future for TipsetRangeSyncer<DB, C>
where
    DB: Blockstore + Sync + Send + 'static,
    C: Consensus,
{
    type Output = Result<(), TipsetRangeSyncerError<C>>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        loop {
            match self.as_mut().tipset_tasks.as_mut().poll_next(cx) {
                Poll::Ready(Some(Ok(_))) => continue,
                Poll::Ready(Some(Err(e))) => return Poll::Ready(Err(e)),
                Poll::Ready(None) => return Poll::Ready(Ok(())),
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

/// Sync headers backwards from the proposed head to the current one, requesting
/// missing tipsets from the network. Once headers are available, download
/// messages going forward on the chain and validate each extension. Finally set
/// the proposed head as the heaviest tipset.
#[allow(clippy::too_many_arguments)]
fn sync_tipset_range<DB: Blockstore + Clone + Sync + Send + 'static, C: Consensus>(
    proposed_head: Arc<Tipset>,
    current_head: Arc<Tipset>,
    tracker: crate::chain_sync::chain_muxer::WorkerState,
    tipset_range_length: u64,
    consensus: Arc<C>,
    state_manager: Arc<StateManager<DB>>,
    chain_store: Arc<ChainStore<DB>>,
    network: SyncNetworkContext<DB>,
    bad_block_cache: Arc<BadBlockCache>,
    genesis: Arc<Tipset>,
) -> TipsetRangeSyncerFuture<C> {
    Box::pin(async move {
        tracker
            .write()
            .init(current_head.clone(), proposed_head.clone());

        let parent_tipsets = match sync_headers_in_reverse(
            tracker.clone(),
            tipset_range_length,
            proposed_head.clone(),
            &current_head,
            &bad_block_cache,
            &chain_store,
            network.clone(),
        )
        .await
        {
            Ok(parent_tipsets) => parent_tipsets,
            Err(why) => {
                tracker.write().error(why.to_string());
                return Err(why);
            }
        };

        // Persist the blocks from the synced Tipsets into the store
        tracker.write().set_stage(SyncStage::Headers);
        let headers: Vec<&BlockHeader> = parent_tipsets.iter().flat_map(|t| t.blocks()).collect();
        if let Err(why) = persist_objects(chain_store.blockstore(), &headers) {
            tracker.write().error(why.to_string());
            return Err(why.into());
        };

        //  Sync and validate messages from the tipsets
        tracker.write().set_stage(SyncStage::Messages);
        if let Err(why) = sync_messages_check_state(
            tracker.clone(),
            consensus,
            state_manager,
            network,
            chain_store.clone(),
            &bad_block_cache,
            parent_tipsets,
            &genesis,
            InvalidBlockStrategy::Strict,
        )
        .await
        {
            error!("Sync messages check state failed for tipset range");
            tracker.write().error(why.to_string());
            return Err(why);
        };
        tracker.write().set_stage(SyncStage::Complete);

        // At this point the head is synced and it can be set in the store as the
        // heaviest
        debug!(
            "Tipset range successfully verified: EPOCH = [{}, {}], HEAD_KEY = {:?}",
            proposed_head.epoch(),
            current_head.epoch(),
            proposed_head.key()
        );
        if let Err(why) = chain_store.put_tipset::<C>(&proposed_head) {
            error!(
                "Putting tipset range head [EPOCH = {}, KEYS = {:?}] in the store failed: {}",
                proposed_head.epoch(),
                proposed_head.key(),
                why
            );
            return Err(why.into());
        };
        Ok(())
    })
}

/// Download headers between the proposed head and the current one available
/// locally. If they turn out to be on different forks, download more headers up
/// to a certain limit to try to find a common ancestor.
async fn sync_headers_in_reverse<DB: Blockstore + Clone + Sync + Send + 'static, C: Consensus>(
    tracker: crate::chain_sync::chain_muxer::WorkerState,
    tipset_range_length: u64,
    proposed_head: Arc<Tipset>,
    current_head: &Tipset,
    bad_block_cache: &BadBlockCache,
    chain_store: &ChainStore<DB>,
    network: SyncNetworkContext<DB>,
) -> Result<Vec<Arc<Tipset>>, TipsetRangeSyncerError<C>> {
    let mut parent_blocks: Vec<Cid> = vec![];
    let mut parent_tipsets = Vec::with_capacity(tipset_range_length as usize + 1);
    parent_tipsets.push(proposed_head.clone());
    tracker.write().set_epoch(current_head.epoch());

    let total_size = proposed_head.epoch() - current_head.epoch();
    let pb = ProgressBar::new(total_size as u64);
    pb.message("Downloading headers ");
    pb.set_max_refresh_rate(Some(std::time::Duration::from_millis(500)));

    'sync: loop {
        // Unwrapping is safe here because the tipset vector always
        // has at least one element
        let oldest_parent = parent_tipsets.last().unwrap();
        let work_to_be_done = oldest_parent.epoch() - current_head.epoch();
        pb.set((work_to_be_done - total_size).unsigned_abs());
        validate_tipset_against_cache(bad_block_cache, oldest_parent.parents(), &parent_blocks)?;

        // Check if we are at the end of the range
        if oldest_parent.epoch() <= current_head.epoch() {
            // Current tipset epoch is less than or equal to the epoch of
            // Tipset we a synchronizing toward, stop.
            break;
        }
        // Attempt to load the parent tipset from local store
        if let Ok(tipset) = chain_store.tipset_from_keys(oldest_parent.parents()) {
            parent_blocks.extend_from_slice(tipset.cids());
            parent_tipsets.push(tipset);
            continue;
        }

        // TODO: Tweak request window when socket frame is tested
        let epoch_diff = oldest_parent.epoch() - current_head.epoch();
        let window = min(epoch_diff, MAX_TIPSETS_TO_REQUEST as i64);
        let network_tipsets = network
            .chain_exchange_headers(None, oldest_parent.parents(), window as u64)
            .await
            .map_err(TipsetRangeSyncerError::NetworkTipsetQueryFailed)?;

        for tipset in network_tipsets {
            // Break if have already traversed the entire tipset range
            if tipset.epoch() < current_head.epoch() {
                break 'sync;
            }
            validate_tipset_against_cache(bad_block_cache, tipset.key(), &parent_blocks)?;
            parent_blocks.extend_from_slice(tipset.cids());
            tracker.write().set_epoch(tipset.epoch());
            parent_tipsets.push(tipset);
        }
    }
    drop(pb);

    // Unwrapping is safe here because we assume that the tipset
    // vector was initialized with a tipset that will not be removed
    let oldest_tipset = parent_tipsets.last().unwrap().clone();
    // Determine if the local chain was a fork.
    // If it was, then sync the fork tipset range by iteratively walking back
    // from the oldest tipset synced until we find a common ancestor
    if oldest_tipset.parents() != current_head.parents() {
        info!("Fork detected, searching for a common ancestor between the local chain and the network chain");
        const FORK_LENGTH_THRESHOLD: u64 = 500;
        let fork_tipsets = network
            .chain_exchange_headers(None, oldest_tipset.parents(), FORK_LENGTH_THRESHOLD)
            .await
            .map_err(TipsetRangeSyncerError::NetworkTipsetQueryFailed)?;
        let mut potential_common_ancestor = chain_store.tipset_from_keys(current_head.parents())?;
        let mut i = 0;
        let mut fork_length = 1;
        while i < fork_tipsets.len() {
            if fork_tipsets[i].epoch() == 0 {
                return Err(TipsetRangeSyncerError::ForkAtGenesisBlock(format!(
                    "{:?}",
                    oldest_tipset.cids()
                )));
            }
            if potential_common_ancestor == fork_tipsets[i] {
                // Remove elements from the vector since the Drain
                // iterator is immediately dropped
                let mut fork_tipsets = fork_tipsets;
                fork_tipsets.drain((i + 1)..);
                parent_tipsets.extend_from_slice(&fork_tipsets);
                break;
            }

            // If the potential common ancestor has an epoch which
            // is lower than the current fork tipset under evaluation
            // move to the next iteration without updated the potential common ancestor
            if potential_common_ancestor.epoch() < fork_tipsets[i].epoch() {
                i += 1;
            } else {
                fork_length += 1;
                // Increment the fork length and enforce the fork length check
                if fork_length > FORK_LENGTH_THRESHOLD {
                    return Err(TipsetRangeSyncerError::ChainForkLengthExceedsMaximum);
                }
                // If we have not found a common ancestor by the last iteration, then return an
                // error
                if i == (fork_tipsets.len() - 1) {
                    return Err(TipsetRangeSyncerError::ChainForkLengthExceedsFinalityThreshold);
                }
                potential_common_ancestor =
                    chain_store.tipset_from_keys(potential_common_ancestor.parents())?;
            }
        }
    }
    Ok(parent_tipsets)
}

#[allow(clippy::too_many_arguments)]
fn sync_tipset<DB: Blockstore + Clone + Sync + Send + 'static, C: Consensus>(
    proposed_head: Arc<Tipset>,
    consensus: Arc<C>,
    state_manager: Arc<StateManager<DB>>,
    chain_store: Arc<ChainStore<DB>>,
    network: SyncNetworkContext<DB>,
    bad_block_cache: Arc<BadBlockCache>,
    genesis: Arc<Tipset>,
) -> TipsetRangeSyncerFuture<C> {
    Box::pin(async move {
        // Persist the blocks from the proposed tipsets into the store
        let headers: Vec<&BlockHeader> = proposed_head.blocks().iter().collect();
        persist_objects(chain_store.blockstore(), &headers)?;

        // Sync and validate messages from the tipsets
        if let Err(e) = sync_messages_check_state(
            // Include a dummy WorkerState
            crate::chain_sync::chain_muxer::WorkerState::default(),
            consensus,
            state_manager,
            network,
            chain_store.clone(),
            &bad_block_cache,
            vec![proposed_head.clone()],
            &genesis,
            InvalidBlockStrategy::Forgiving,
        )
        .await
        {
            warn!("Sync messages check state failed for single tipset");
            return Err(e);
        }

        // Add the tipset to the store. The tipset will be expanded with other blocks
        // with the same [epoch, parents] before updating the heaviest Tipset in
        // the store.
        if let Err(why) = chain_store.put_tipset::<C>(&proposed_head) {
            error!(
                "Putting tipset [EPOCH = {}, KEYS = {:?}] in the store failed: {}",
                proposed_head.epoch(),
                proposed_head.key(),
                why
            );
            return Err(why.into());
        };
        Ok(())
    })
}

async fn fetch_batch<DB: Blockstore + Clone + Send + Sync + 'static, C: Consensus>(
    batch: &[Arc<Tipset>],
    network: &SyncNetworkContext<DB>,
    chainstore: &ChainStore<DB>,
    s: &flume::Sender<FullTipset>,
) -> Result<(), TipsetRangeSyncerError<C>> {
    // Tipsets in `batch` are already in chronological order
    if let Some(head) = batch.last() {
        let epoch = head.epoch();
        let len = batch.len();

        debug!("ChainExchange message sync tipsets: epoch: {epoch}, len: {len}");

        let compacted_messages = network
            .chain_exchange_messages(None, head.key(), len as u64)
            .await
            .map_err(TipsetRangeSyncerError::NetworkMessageQueryFailed)?;

        // Since the bundle only has messages, we have to put the headers in them
        for (messages, tipset) in compacted_messages.into_iter().rev().zip(batch.iter()) {
            // Construct full tipset from fetched messages
            let bundle = TipsetBundle {
                blocks: tipset.blocks().to_vec(),
                messages: Some(messages),
            };

            let full_tipset = FullTipset::try_from(&bundle)
                .map_err(TipsetRangeSyncerError::GeneratingTipsetFromTipsetBundle)?;

            // Persist the messages in the store
            if let Some(m) = bundle.messages {
                crate::chain::persist_objects(chainstore.blockstore(), &m.bls_msgs)?;
                crate::chain::persist_objects(chainstore.blockstore(), &m.secp_msgs)?;
            } else {
                warn!("ChainExchange request for messages returned null messages");
            }

            s.send_async(full_tipset).await?;
        }
    }

    Ok(())
}

/// Going forward along the tipsets, try to load the messages in them from the
/// `BlockStore`, or download them from the network, then validate the full
/// tipset on each epoch.
#[allow(clippy::too_many_arguments)]
async fn sync_messages_check_state<DB: Blockstore + Clone + Send + Sync + 'static, C: Consensus>(
    tracker: crate::chain_sync::chain_muxer::WorkerState,
    consensus: Arc<C>,
    state_manager: Arc<StateManager<DB>>,
    network: SyncNetworkContext<DB>,
    chainstore: Arc<ChainStore<DB>>,
    bad_block_cache: &BadBlockCache,
    tipsets: Vec<Arc<Tipset>>,
    genesis: &Tipset,
    invalid_block_strategy: InvalidBlockStrategy,
) -> Result<(), TipsetRangeSyncerError<C>> {
    let task_chainstore = chainstore.clone();
    let request_window = state_manager.chain_config().request_window;
    // Spawn a background task for the chain_exchange message requests

    let (s, r) = flume::bounded(request_window * 4);
    let handle = tokio::task::spawn(async move {
        let mut batch: Vec<Arc<Tipset>> = Vec::with_capacity(request_window);

        // Visit tipsets in chronological order
        for tipset in tipsets.into_iter().rev() {
            // If the current tipset batch is empty and we already have the
            // messages for the current tipset, skip the download and send
            // it directly to the validator.
            if batch.is_empty() {
                if let Some(full_tipset) = task_chainstore.fill_tipset(&tipset) {
                    s.send_async(full_tipset).await?;
                    continue;
                }
            }

            // Request tipset messages via chain_exchange
            batch.push(tipset);
            if batch.len() == request_window {
                fetch_batch(&batch, &network, &task_chainstore, &s).await?;
                batch.clear();
            }
        }
        // Fetch last batch
        fetch_batch(&batch, &network, &task_chainstore, &s).await?;

        Ok(())
    });

    // Validation loop
    while let Ok(full_tipset) = r.recv_async().await {
        let current_epoch = full_tipset.epoch();
        {
            let _timer = metrics::TIPSET_PROCESSING_TIME.start_timer();
            validate_tipset::<_, C>(
                consensus.clone(),
                state_manager.clone(),
                &chainstore,
                bad_block_cache,
                full_tipset.clone(),
                genesis,
                invalid_block_strategy,
            )
            .await?;
        }
        chainstore.set_heaviest_tipset(Arc::new(full_tipset.into_tipset()))?;
        tracker.write().set_epoch(current_epoch);
        metrics::LAST_VALIDATED_TIPSET_EPOCH.set(current_epoch as u64);
    }

    handle.await?
}

/// Validates full blocks in the tipset in parallel (since the messages are not
/// executed), adding the successful ones to the tipset tracker, and the failed
/// ones to the bad block cache, depending on strategy. Any bad block fails
/// validation.
async fn validate_tipset<DB: Blockstore + Clone + Send + Sync + 'static, C: Consensus>(
    consensus: Arc<C>,
    state_manager: Arc<StateManager<DB>>,
    chainstore: &ChainStore<DB>,
    bad_block_cache: &BadBlockCache,
    full_tipset: FullTipset,
    genesis: &Tipset,
    invalid_block_strategy: InvalidBlockStrategy,
) -> Result<(), TipsetRangeSyncerError<C>> {
    if full_tipset.key().eq(genesis.key()) {
        trace!("Skipping genesis tipset validation");
        return Ok(());
    }

    let epoch = full_tipset.epoch();
    let full_tipset_key = full_tipset.key().clone();

    let mut validations = FuturesUnordered::new();
    let blocks = full_tipset.into_blocks();

    info!(
        "Validating tipset: EPOCH = {epoch}, N blocks = {}",
        blocks.len()
    );
    debug!("Tipset keys: {:?}", full_tipset_key.cids);

    for b in blocks {
        let validation_fn = tokio::task::spawn(validate_block::<_, C>(
            consensus.clone(),
            state_manager.clone(),
            Arc::new(b),
        ));
        validations.push(validation_fn);
    }

    while let Some(result) = validations.next().await {
        match result? {
            Ok(block) => {
                chainstore.add_to_tipset_tracker(block.header());
            }
            Err((cid, why)) => {
                warn!(
                    "Validating block [CID = {}] in EPOCH = {} failed: {}",
                    cid.clone(),
                    epoch,
                    why
                );
                // Only do bad block accounting if the function was called with
                // `is_strict` = true
                if let InvalidBlockStrategy::Strict = invalid_block_strategy {
                    match &why {
                        TipsetRangeSyncerError::TimeTravellingBlock(_, _)
                        | TipsetRangeSyncerError::TipsetParentNotFound(_) => (),
                        why => {
                            bad_block_cache.put(cid, why.to_string());
                        }
                    }
                }
                return Err(why);
            }
        }
    }
    Ok(())
}

/// Validate the block according to the rules specific to the consensus being
/// used, and the common rules that pertain to the assumptions of the
/// `ChainSync` protocol.
///
/// Returns the validated block if `Ok`.
/// Returns the block CID (for marking bad) and `Error` if invalid (`Err`).
///
/// Common validation includes:
/// * Sanity checks
/// * Clock drifts
/// * Signatures
/// * Message inclusion (fees, sequences)
/// * Parent related fields: base fee, weight, the state root
/// * NB: This is where the messages in the *parent* tipset are executed.
///
/// Consensus specific validation should include:
/// * Checking that the messages in the block correspond to the agreed upon
///   total ordering
/// * That the block is a deterministic derivative of the underlying consensus
async fn validate_block<DB: Blockstore + Clone + Sync + Send + 'static, C: Consensus>(
    consensus: Arc<C>,
    state_manager: Arc<StateManager<DB>>,
    block: Arc<Block>,
) -> Result<Arc<Block>, (Cid, TipsetRangeSyncerError<C>)> {
    trace!(
        "Validating block: epoch = {}, weight = {}, key = {}",
        block.header().epoch(),
        block.header().weight(),
        block.header().cid(),
    );
    let chain_store = state_manager.chain_store().clone();
    let block_cid = block.cid();

    // Check block validation cache in store
    let is_validated = chain_store.is_block_validated(block_cid);
    if is_validated {
        return Ok(block);
    }

    let _timer = metrics::BLOCK_VALIDATION_TIME.start_timer();

    let header = block.header();

    // Check to ensure all optional values exist
    block_sanity_checks(header).map_err(|e| (*block_cid, e))?;
    block_timestamp_checks(header).map_err(|e| (*block_cid, e))?;

    let base_tipset = chain_store
        .tipset_from_keys(header.parents())
        // The parent tipset will always be there when calling validate_block
        // as part of the sync_tipset_range flow because all of the headers in the range
        // have been committed to the store. When validate_block is called from sync_tipset
        // this guarantee does not exist, so we create a specific error to inform the caller
        // not to add this block to the bad blocks cache.
        .map_err(|why| {
            (
                *block_cid,
                TipsetRangeSyncerError::TipsetParentNotFound(why),
            )
        })?;

    // Retrieve lookback tipset for validation
    let lookback_state = state_manager
        .get_lookback_tipset_for_round(base_tipset.clone(), block.header().epoch())
        .map_err(|e| (*block_cid, e.into()))
        .map(|(_, s)| Arc::new(s))?;

    // Work address needed for async validations, so necessary
    // to do sync to avoid duplication
    let work_addr = state_manager
        .get_miner_work_addr(*lookback_state, header.miner_address())
        .map_err(|e| (*block_cid, e.into()))?;

    // Async validations
    let validations = FuturesUnordered::new();

    // Check block messages
    validations.push(tokio::task::spawn(check_block_messages::<_, C>(
        Arc::clone(&state_manager),
        Arc::clone(&block),
        Arc::clone(&base_tipset),
    )));

    // Base fee check
    let smoke_height = state_manager.chain_config().epoch(Height::Smoke);
    let v_base_tipset = Arc::clone(&base_tipset);
    let v_block_store = state_manager.blockstore().clone();
    let v_block = Arc::clone(&block);
    validations.push(tokio::task::spawn_blocking(move || {
        let _timer = metrics::BLOCK_VALIDATION_TASKS_TIME
            .with_label_values(&[metrics::values::BASE_FEE_CHECK])
            .start_timer();
        let base_fee = crate::chain::compute_base_fee(&v_block_store, &v_base_tipset, smoke_height)
            .map_err(|e| {
                TipsetRangeSyncerError::<C>::Validation(format!("Could not compute base fee: {e}"))
            })?;
        let parent_base_fee = v_block.header.parent_base_fee();
        if base_fee != parent_base_fee.clone() {
            return Err(TipsetRangeSyncerError::<C>::Validation(format!(
                "base fee doesn't match: {parent_base_fee} (header), {base_fee} (computed)"
            )));
        }
        Ok(())
    }));

    // Parent weight calculation check
    let v_block_store = state_manager.blockstore().clone();
    let v_base_tipset = Arc::clone(&base_tipset);
    let weight = header.weight().clone();
    validations.push(tokio::task::spawn_blocking(move || {
        let _timer = metrics::BLOCK_VALIDATION_TASKS_TIME
            .with_label_values(&[metrics::values::PARENT_WEIGHT_CAL])
            .start_timer();
        let calc_weight = C::weight(&v_block_store, &v_base_tipset).map_err(|e| {
            TipsetRangeSyncerError::Calculation(format!("Error calculating weight: {e}"))
        })?;
        if weight != calc_weight {
            return Err(TipsetRangeSyncerError::<C>::Validation(format!(
                "Parent weight doesn't match: {weight} (header), {calc_weight} (computed)"
            )));
        }
        Ok(())
    }));

    // State root and receipt root validations
    let v_state_manager = Arc::clone(&state_manager);
    let v_base_tipset = Arc::clone(&base_tipset);
    let v_block = Arc::clone(&block);
    validations.push(tokio::task::spawn(async move {
        let header = v_block.header();
        let (state_root, receipt_root) = v_state_manager
            .tipset_state(&v_base_tipset)
            .await
            .map_err(|e| {
                TipsetRangeSyncerError::Calculation(format!("Failed to calculate state: {e}"))
            })?;

        if &state_root != header.state_root() {
            return Err(TipsetRangeSyncerError::<C>::Validation(format!(
                "Parent state root did not match computed state: {} (header), {} (computed)",
                header.state_root(),
                state_root,
            )));
        }

        if &receipt_root != header.message_receipts() {
            return Err(TipsetRangeSyncerError::<C>::Validation(format!(
                "Parent receipt root did not match computed root: {} (header), {} (computed)",
                header.message_receipts(),
                receipt_root
            )));
        }
        Ok(())
    }));

    // Block signature check
    let v_block = block.clone();
    validations.push(tokio::task::spawn_blocking(move || {
        let _timer = metrics::BLOCK_VALIDATION_TASKS_TIME
            .with_label_values(&[metrics::values::BLOCK_SIGNATURE_CHECK])
            .start_timer();
        v_block.header().check_block_signature(&work_addr)?;
        Ok(())
    }));

    let v_block = block.clone();
    validations.push(tokio::task::spawn(async move {
        consensus
            .validate_block(state_manager, v_block)
            .map_err(|errs| {
                // NOTE: Concatenating errors here means the wrapper type of error
                // never surfaces, yet we always pay the cost of the generic argument.
                // But there's no reason `validate_block` couldn't return a list of all
                // errors instead of a single one that has all the error messages,
                // removing the caller's ability to distinguish between them.
                let errs = errs.map(|err| TipsetRangeSyncerError::<C>::ConsensusError(err));

                TipsetRangeSyncerError::<C>::concat(errs)
            })
            .await
    }));

    // Collect the errors from the async validations
    if let Err(errs) = collect_errs(validations).await {
        return Err((*block_cid, TipsetRangeSyncerError::<C>::concat(errs)));
    }

    chain_store.mark_block_as_validated(block_cid);

    Ok(block)
}

/// Validate messages in a full block, relative to the parent tipset.
///
/// This includes:
/// * signature checks
/// * gas limits, and prices
/// * account nonce values
/// * the message root in the header
///
/// NB: This loads/computes the state resulting from the execution of the parent
/// tipset.
async fn check_block_messages<DB: Blockstore + Clone + Send + Sync + 'static, C: Consensus>(
    state_manager: Arc<StateManager<DB>>,
    block: Arc<Block>,
    base_tipset: Arc<Tipset>,
) -> Result<(), TipsetRangeSyncerError<C>> {
    let network_version = state_manager
        .chain_config()
        .network_version(block.header.epoch());

    // Do the initial loop here
    // check block message and signatures in them
    let mut pub_keys = Vec::new();
    let mut cids = Vec::new();
    for m in block.bls_msgs() {
        let pk = StateManager::get_bls_public_key(
            state_manager.blockstore(),
            &m.from,
            *base_tipset.parent_state(),
        )?;
        pub_keys.push(pk);
        cids.push(m.cid().unwrap().to_bytes());
    }

    if let Some(sig) = block.header().bls_aggregate() {
        if !verify_bls_aggregate(
            cids.iter()
                .map(|x| x.as_slice())
                .collect::<Vec<&[u8]>>()
                .as_slice(),
            pub_keys
                .iter()
                .map(|x| &x[..])
                .collect::<Vec<&[u8]>>()
                .as_slice(),
            sig,
        ) {
            return Err(TipsetRangeSyncerError::BlsAggregateSignatureInvalid(
                format!("{sig:?}"),
                format!("{cids:?}"),
            ));
        }
    } else {
        return Err(TipsetRangeSyncerError::BlockWithoutBlsAggregate);
    }

    let price_list = price_list_by_network_version(network_version);
    let mut sum_gas_limit = 0;

    // Check messages for validity
    let mut check_msg = |msg: &Message,
                         account_sequences: &mut HashMap<Address, u64>,
                         tree: &StateTree<&DB>|
     -> Result<(), anyhow::Error> {
        // Phase 1: Syntactic validation
        let min_gas = price_list.on_chain_message(to_vec(msg).unwrap().len());
        valid_for_block_inclusion(msg, min_gas.total(), network_version)
            .map_err(|e| anyhow::anyhow!("{}", e))?;
        sum_gas_limit += msg.gas_limit;
        if sum_gas_limit > BLOCK_GAS_LIMIT {
            anyhow::bail!("block gas limit exceeded");
        }

        // Phase 2: (Partial) Semantic validation
        // Send exists and is an account actor, and sequence is correct
        let sequence: u64 = match account_sequences.get(&msg.from()) {
            Some(sequence) => *sequence,
            None => {
                let actor = tree.get_actor(&msg.from)?.ok_or_else(|| {
                    anyhow::anyhow!(
                        "Failed to retrieve nonce for addr: Actor does not exist in state"
                    )
                })?;
                let network_version = state_manager
                    .chain_config()
                    .network_version(block.header.epoch());
                if !is_valid_for_sending(network_version, &actor) {
                    anyhow::bail!("not valid for sending!");
                }
                actor.sequence
            }
        };

        // Sequence equality check
        if sequence != msg.sequence {
            anyhow::bail!(
                "Message has incorrect sequence (exp: {} got: {})",
                sequence,
                msg.sequence
            );
        }
        account_sequences.insert(msg.from(), sequence + 1);
        Ok(())
    };

    let mut account_sequences: HashMap<Address, u64> = HashMap::default();
    let block_store = state_manager.blockstore();
    let (state_root, _) = state_manager
        .tipset_state(&base_tipset)
        .await
        .map_err(|e| TipsetRangeSyncerError::Calculation(format!("Could not update state: {e}")))?;
    let tree = StateTree::new_from_root(block_store, &state_root).map_err(|e| {
        TipsetRangeSyncerError::Calculation(format!(
            "Could not load from new state root in state manager: {e}"
        ))
    })?;

    // Check validity for BLS messages
    for (i, msg) in block.bls_msgs().iter().enumerate() {
        check_msg(msg, &mut account_sequences, &tree).map_err(|e| {
            TipsetRangeSyncerError::<C>::Validation(format!(
                "Block had invalid BLS message at index {i}: {e}"
            ))
        })?;
    }

    // Check validity for SECP messages
    for (i, msg) in block.secp_msgs().iter().enumerate() {
        check_msg(msg.message(), &mut account_sequences, &tree).map_err(|e| {
            TipsetRangeSyncerError::<C>::Validation(format!(
                "block had an invalid secp message at index {i}: {e}"
            ))
        })?;
        // Resolve key address for signature verification
        let key_addr = state_manager
            .resolve_to_key_addr(&msg.from(), &base_tipset)
            .await
            .map_err(|e| TipsetRangeSyncerError::ResolvingAddressFromMessage(e.to_string()))?;
        // SecP256K1 Signature validation
        msg.signature
            .verify(&msg.message().cid().unwrap().to_bytes(), &key_addr)
            .map_err(TipsetRangeSyncerError::MessageSignatureInvalid)?;
    }

    // Validate message root from header matches message root
    let msg_root =
        TipsetValidator::compute_msg_root(block_store, block.bls_msgs(), block.secp_msgs())
            .map_err(|err| TipsetRangeSyncerError::ComputingMessageRoot(err.to_string()))?;
    if block.header().messages() != &msg_root {
        return Err(TipsetRangeSyncerError::BlockMessageRootInvalid(
            format!("{:?}", block.header().messages()),
            format!("{msg_root:?}"),
        ));
    }

    Ok(())
}

/// Checks optional values in header.
///
/// It only looks for fields which are common to all consensus types.
fn block_sanity_checks<C: Consensus>(
    header: &BlockHeader,
) -> Result<(), TipsetRangeSyncerError<C>> {
    if header.signature().is_none() {
        return Err(TipsetRangeSyncerError::BlockWithoutSignature);
    }
    if header.bls_aggregate().is_none() {
        return Err(TipsetRangeSyncerError::BlockWithoutBlsAggregate);
    }
    Ok(())
}

/// Check the clock drift.
fn block_timestamp_checks<C: Consensus>(
    header: &BlockHeader,
) -> Result<(), TipsetRangeSyncerError<C>> {
    // TODO: Time should come from a component we control, for testing.
    let time_now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Retrieved system time before UNIX epoch")
        .as_secs();
    if header.timestamp() > time_now + ALLOWABLE_CLOCK_DRIFT {
        return Err(TipsetRangeSyncerError::TimeTravellingBlock(
            time_now,
            header.timestamp(),
        ));
    } else if header.timestamp() > time_now {
        warn!(
            "Got block from the future, but within clock drift threshold, {} > {}",
            header.timestamp(),
            time_now
        );
    }
    Ok(())
}

/// Check if any CID in `tipset` is a known bad block.
/// If so, add all their descendants to the bad block cache and return an error.
fn validate_tipset_against_cache<C: Consensus>(
    bad_block_cache: &BadBlockCache,
    tipset: &TipsetKeys,
    descendant_blocks: &[Cid],
) -> Result<(), TipsetRangeSyncerError<C>> {
    for cid in tipset.cids() {
        if let Some(reason) = bad_block_cache.get(cid) {
            for block_cid in descendant_blocks {
                bad_block_cache.put(*block_cid, format!("chain contained {cid}"));
            }
            return Err(TipsetRangeSyncerError::TipsetRangeWithBadBlock(
                *cid, reason,
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod test {
    use crate::blocks::{BlockHeader, ElectionProof, Ticket, Tipset};
    use crate::json::vrf::VRFProof;
    use crate::shim::address::Address;
    use cid::Cid;
    use num_bigint::BigInt;

    use super::*;

    pub fn mock_block(id: u64, weight: u64, ticket_sequence: u64) -> BlockHeader {
        let addr = Address::new_id(id);
        let cid =
            Cid::try_from("bafyreicmaj5hhoy5mgqvamfhgexxyergw7hdeshizghodwkjg6qmpoco7i").unwrap();

        let fmt_str = format!("===={ticket_sequence}=====");
        let ticket = Ticket::new(VRFProof::new(fmt_str.clone().into_bytes()));
        let election_proof = ElectionProof {
            win_count: 0,
            vrfproof: VRFProof::new(fmt_str.into_bytes()),
        };
        let weight_inc = BigInt::from(weight);
        BlockHeader::builder()
            .miner_address(addr)
            .election_proof(Some(election_proof))
            .ticket(Some(ticket))
            .message_receipts(cid)
            .messages(cid)
            .state_root(cid)
            .weight(weight_inc)
            .build()
            .unwrap()
    }

    #[test]
    pub fn test_heaviest_weight() {
        // ticket_sequence are chosen so that Ticket(b3) < Ticket(b1)

        let b1 = mock_block(1234561, 10, 2);
        let ts1 = Tipset::from(b1);

        let b2 = mock_block(1234563, 9, 1);
        let ts2 = Tipset::from(b2);

        let b3 = mock_block(1234562, 10, 1);
        let ts3 = Tipset::from(b3);

        let mut tsg = TipsetGroup::new(Arc::new(ts1));
        assert!(tsg.try_add_tipset(Arc::new(ts2)).is_none());
        assert!(tsg.try_add_tipset(Arc::new(ts3)).is_none());

        let (index, weight) = tsg.heaviest_weight();
        assert_eq!(index, 2);
        assert_eq!(weight, &BigInt::from(10));
    }
}
