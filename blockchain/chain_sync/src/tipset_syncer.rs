// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::cmp::min;
use std::collections::{HashMap, HashSet};
use std::convert::TryFrom;
use std::error::Error as StdError;
use std::marker::PhantomData;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use async_std::future::Future;
use async_std::pin::Pin;
use async_std::stream::{Stream, StreamExt};
use async_std::task::{self, Context, Poll};
use futures::stream::FuturesUnordered;
use log::{debug, error, info, trace, warn};
use num_bigint::BigInt;
use thiserror::Error;

use crate::bad_block_cache::BadBlockCache;
use crate::metrics;
use crate::network_context::SyncNetworkContext;
use crate::sync_state::SyncStage;
use crate::validation::TipsetValidator;
use actor::{is_account_actor, power};
use address::Address;
use beacon::{Beacon, BeaconEntry, BeaconSchedule, IGNORE_DRAND_VAR};
use blocks::{Block, BlockHeader, Error as ForestBlockError, FullTipset, Tipset, TipsetKeys};
use chain::Error as ChainStoreError;
use chain::{persist_objects, ChainStore};
use cid::Cid;
use clock::ChainEpoch;
use crypto::{verify_bls_aggregate, DomainSeparationTag};
use encoding::Cbor;
use encoding::Error as ForestEncodingError;
use fil_types::{
    verifier::ProofVerifier, NetworkVersion, Randomness, ALLOWABLE_CLOCK_DRIFT, BLOCK_GAS_LIMIT,
    TICKET_RANDOMNESS_LOOKBACK,
};
use forest_libp2p::chain_exchange::TipsetBundle;
use interpreter::price_list_by_epoch;
use ipld_blockstore::BlockStore;
use message::{Message, UnsignedMessage};
use networks::{get_network_version_default, BLOCK_DELAY_SECS, UPGRADE_SMOKE_HEIGHT};
use state_manager::Error as StateManagerError;
use state_manager::StateManager;
use state_tree::StateTree;

const MAX_TIPSETS_TO_REQUEST: u64 = 100;

#[derive(Debug, Error)]
pub enum TipsetProcessorError {
    #[error("TipsetRangeSyncer error: {0}")]
    TipsetRangeSyncer(#[from] TipsetRangeSyncerError),
    #[error("Tipset stream closed")]
    TipsetStreamClosed,
    #[error("Tipset has already been synced")]
    TipsetAlreadySynced,
}

#[derive(Debug, Error)]
pub enum TipsetRangeSyncerError {
    #[error("Tipset added to range syncer does share the same epoch and parents")]
    InvalidTipsetAdded,
    #[error("Tipset range length is less than 0")]
    InvalidTipsetRangeLength,
    #[error("Provided tiset does not match epoch for the range")]
    InvalidTipsetEpoch,
    #[error("Provided tipset does not match parent for the range")]
    InvalidTipsetParent,
    #[error("Block must have an election proof included in tipset")]
    BlockWithoutElectionProof,
    #[error("Block must have a signature")]
    BlockWithoutSignature,
    #[error("Block without BLS aggregate signature")]
    BlockWithoutBlsAggregate,
    #[error("Block without ticket")]
    BlockWithoutTicket,
    #[error("Block had the wrong timestamp: {0} != {1}")]
    UnequalBlockTimestamps(u64, u64),
    #[error("Block received from the future: now = {0}, block = {1}")]
    TimeTravellingBlock(u64, u64),
    #[error("Tipset range contains bad block [block = {0}]: {1}")]
    TipsetRangeWithBadBlock(Cid, String),
    #[error("Tipset without ticket to verify")]
    TipsetWithoutTicket,
    #[error("Validation error: {0}")]
    Validation(String),
    #[error("Processing error: {0}")]
    Calculation(String),
    #[error("Chain store error: {0}")]
    ChainStore(#[from] ChainStoreError),
    #[error("StateManager error: {0}")]
    StateManager(#[from] StateManagerError),
    #[error("Encoding error: {0}")]
    ForestEncoding(#[from] ForestEncodingError),
    #[error("Winner election proof verification failed: {0}")]
    WinnerElectionProofVerificationFailed(String),
    #[error("Block miner was slashed or is invalid")]
    InvalidOrSlashedMiner,
    #[error("Miner power not available for miner address")]
    MinerPowerNotAvailable,
    #[error("Miner claimed wrong number of wins: miner = {0}, computed = {1}")]
    MinerWinClaimsIncorrect(i64, i64),
    #[error("Drawing chain randomness failed: {0}")]
    DrawingChainRandomness(String),
    #[error("Miner isn't elligible to mine")]
    MinerNotEligibleToMine,
    #[error("Block error: {0}")]
    BlockError(#[from] ForestBlockError),
    #[error("Chain fork length exceeds the maximum")]
    ChainForkLengthExceedsMaximum,
    #[error("Chain fork length exceeds finality threshold")]
    ChainForkLengthExceedsFinalityThreshold,
    #[error("Chain for block forked from local chain at genesis, refusing to sync block: {0}")]
    ForkAtGenesisBlock(String),
    #[error("Querying miner power failed: {0}")]
    MinerPowerUnavailable(String),
    #[error("Power actor not found")]
    PowerActorUnavailable,
    #[error("Querying tipsets from the network failed: {0}")]
    NetworkTipsetQueryFailed(String),
    #[error("Query tipset messages from the network failed: {0}")]
    NetworkMessageQueryFailed(String),
    #[error("Verifying VRF failed: {0}")]
    VrfValidation(String),
    #[error("BLS aggregate signature {0} was invalid for msgs {1}")]
    BlsAggregateSignatureInvalid(String, String),
    #[error("Message signature invalid: {0}")]
    MessageSignatureInvalid(String),
    #[error("Block message root does not match: expected {0}, computed {1}")]
    BlockMessageRootInvalid(String, String),
    #[error("Message validation for msg {0} failed: {1}")]
    BlockMessageValidationFailed(usize, String),
    #[error("Computing message root failed: {0}")]
    ComputingMessageRoot(String),
    #[error("Resolving address from message failed: {0}")]
    ResolvingAddressFromMessage(String),
    #[error("Generating Tipset from bundle failed: {0}")]
    GeneratingTipsetFromTipsetBundle(String),
    #[error("[INSECURE-POST-VALIDATION] {0}")]
    InsecurePostValidation(String),
    #[error("Loading tipset parent from the store failed: {0}")]
    TipsetParentNotFound(ChainStoreError),
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
        if self.tipsets.iter().any(|ts| tipset.key().eq(&ts.key())) {
            return Some(tipset);
        }
        self.tipsets.push(tipset);
        None
    }

    fn take_heaviest_tipset(&mut self) -> Option<Arc<Tipset>> {
        self.tipsets
            .iter()
            .enumerate()
            .max_by_key(|(_idx, ts)| ts.weight())
            .map(|(idx, _)| idx)
            .map(|idx| self.tipsets.swap_remove(idx))
    }

    fn heaviest_weight(&self) -> BigInt {
        // Unwrapping is safe because we initialize the struct with at least one tipset
        self.tipsets
            .iter()
            .map(|ts| ts.weight().clone())
            .max()
            .unwrap()
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
        self.heaviest_weight() > other.heaviest_weight()
    }

    fn tipsets(self) -> Vec<Arc<Tipset>> {
        self.tipsets
    }
}

/// The TipsetProcessor receives and prioritizes a stream of Tipsets
/// for syncing from the ChainMuxer and the SyncSubmitBlock API before syncing.
/// Each unique Tipset, by epoch and parents, is mapped into a Tipset range which will be synced into the Chain Store.
pub(crate) struct TipsetProcessor<DB, TBeacon, V> {
    state: TipsetProcessorState<DB, TBeacon, V>,
    tracker: crate::chain_muxer::WorkerState,
    /// Tipsets pushed into this stream _must_ be validated beforehand by the TipsetValidator
    tipsets: Pin<Box<dyn Stream<Item = Arc<Tipset>> + Send>>,
    state_manager: Arc<StateManager<DB>>,
    beacon: Arc<BeaconSchedule<TBeacon>>,
    network: SyncNetworkContext<DB>,
    chain_store: Arc<ChainStore<DB>>,
    bad_block_cache: Arc<BadBlockCache>,
    genesis: Arc<Tipset>,
    verifier: PhantomData<V>,
}

impl<DB, TBeacon, V> TipsetProcessor<DB, TBeacon, V>
where
    TBeacon: Beacon + Sync + Send + 'static,
    DB: BlockStore + Sync + Send + 'static,
    V: ProofVerifier + Sync + Send + 'static + Unpin,
{
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        tracker: crate::chain_muxer::WorkerState,
        tipsets: Pin<Box<dyn Stream<Item = Arc<Tipset>> + Send>>,
        state_manager: Arc<StateManager<DB>>,
        beacon: Arc<BeaconSchedule<TBeacon>>,
        network: SyncNetworkContext<DB>,
        chain_store: Arc<ChainStore<DB>>,
        bad_block_cache: Arc<BadBlockCache>,
        genesis: Arc<Tipset>,
    ) -> Self {
        Self {
            state: TipsetProcessorState::Idle,
            tracker,
            tipsets,
            state_manager,
            beacon,
            network,
            chain_store,
            bad_block_cache,
            genesis,
            verifier: Default::default(),
        }
    }

    fn find_range(
        &self,
        mut tipset_group: TipsetGroup,
    ) -> TipsetProcessorFuture<TipsetRangeSyncer<DB, TBeacon, V>, TipsetProcessorError> {
        let state_manager = self.state_manager.clone();
        let chain_store = self.chain_store.clone();
        let beacon = self.beacon.clone();
        let network = self.network.clone();
        let bad_block_cache = self.bad_block_cache.clone();
        let tracker = self.tracker.clone();
        let genesis = self.genesis.clone();
        Box::pin(async move {
            // Define the low end of the range
            // Unwrapping is safe here because the store always has at least one tipset
            let current_head = chain_store.heaviest_tipset().await.unwrap();
            // Unwrapping is safe here because we assume that the
            // tipset group contains at least one tipset
            let proposed_head = tipset_group.take_heaviest_tipset().unwrap();

            if current_head.key().eq(proposed_head.key()) {
                return Err(TipsetProcessorError::TipsetAlreadySynced);
            }

            let mut tipset_range_syncer = TipsetRangeSyncer::new(
                tracker,
                proposed_head,
                current_head,
                state_manager,
                beacon,
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

enum TipsetProcessorState<DB, TBeacon, V> {
    Idle,
    FindRange {
        range_finder:
            TipsetProcessorFuture<TipsetRangeSyncer<DB, TBeacon, V>, TipsetProcessorError>,
        epoch: i64,
        parents: TipsetKeys,
        current_sync: Option<TipsetGroup>,
        next_sync: Option<TipsetGroup>,
    },
    SyncRange {
        range_syncer: Pin<Box<TipsetRangeSyncer<DB, TBeacon, V>>>,
        next_sync: Option<TipsetGroup>,
    },
}

impl<DB, TBeacon, V> Future for TipsetProcessor<DB, TBeacon, V>
where
    TBeacon: Beacon + Sync + Send + 'static,
    DB: BlockStore + Sync + Send + 'static,
    V: ProofVerifier + Sync + Send + 'static + Unpin,
{
    type Output = Result<(), TipsetProcessorError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        trace!("Polling TipsetProcessor");

        // TODO: Determine if polling the tipset stream before the state machine
        //       introduces a DOS attack vector where peers send duplicate, valid tipsets over
        //       GossipSub to divert resources away from syncing tipset ranges.
        // First, gather the tipsets off of the channel. Reading off the receiver will return immediately.
        // Ensure that the task will wake up when the stream has a new item by registering it for wakeup.
        // As a tipset is received through the stream we assume:
        //   1. Tipset has at least 1 block
        //   2. Tipset epoch is not behind the current max epoch in the store
        //   3. Tipset is heavier than the heaviest tipset in the store at the time when it was queued
        //   4. Tipset message roots were calculated and integrity checks were run

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
                    return Poll::Ready(Err(TipsetProcessorError::TipsetStreamClosed));
                }
                // The current task is registered for wakeup when this
                // stream has a new item available. Break here to make
                // forward progress on syncing before polling the stream again.
                Poll::Pending => break,
            }
        }

        trace!("Tipsets received through stream: {}", grouped_tipsets.len());

        // Consume the tipsets read off of the stream and attempt to update the state machine
        match self.state {
            TipsetProcessorState::Idle => {
                // Set the state to FindRange if we have a tipset to sync towards
                // Consume the tipsets received, start syncing the heaviest tipset group, and discard the rest
                if let Some(((epoch, parents), heaviest_tipset_group)) = grouped_tipsets
                    .into_iter()
                    .max_by_key(|(_, group)| group.heaviest_weight())
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
                    .map(|(_, group)| group)
                    .max_by_key(|group| group.heaviest_weight())
                {
                    // Find the heaviest tipset group and either merge it with the
                    // tipset group in the next_sync or replace it.
                    match next_sync {
                        None => *next_sync = Some(heaviest_tipset_group),
                        Some(ns) => {
                            if ns.is_mergeable(&heaviest_tipset_group) {
                                // Both tipsets groups have the same epoch & parents, so merge them
                                ns.merge(heaviest_tipset_group);
                            } else if heaviest_tipset_group.is_heavier_than(&ns) {
                                // The tipset group received is heavier than the one saved, replace it.
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
                    .map(|(_, group)| group)
                    .max_by_key(|group| group.heaviest_weight())
                {
                    // Find the heaviest tipset group and either merge it with the
                    // tipset group in the next_sync or replace it.
                    match next_sync {
                        None => *next_sync = Some(heaviest_tipset_group),
                        Some(ns) => {
                            if ns.is_mergeable(&heaviest_tipset_group) {
                                // Both tipsets groups have the same epoch & parents, so merge them
                                ns.merge(heaviest_tipset_group);
                            } else if heaviest_tipset_group.is_heavier_than(&ns) {
                                // The tipset group received is heavier than the one saved, replace it.
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
                            TipsetProcessorError::TipsetAlreadySynced
                            | TipsetProcessorError::TipsetRangeSyncer(
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

type TipsetRangeSyncerFuture =
    Pin<Box<dyn Future<Output = Result<(), TipsetRangeSyncerError>> + Send>>;

pub(crate) struct TipsetRangeSyncer<DB, TBeacon, V> {
    pub proposed_head: Arc<Tipset>,
    pub current_head: Arc<Tipset>,
    tipsets_included: HashSet<TipsetKeys>,
    tipset_tasks: Pin<Box<FuturesUnordered<TipsetRangeSyncerFuture>>>,
    state_manager: Arc<StateManager<DB>>,
    beacon: Arc<BeaconSchedule<TBeacon>>,
    network: SyncNetworkContext<DB>,
    chain_store: Arc<ChainStore<DB>>,
    bad_block_cache: Arc<BadBlockCache>,
    genesis: Arc<Tipset>,
    verifier: PhantomData<V>,
}

impl<DB, TBeacon, V> TipsetRangeSyncer<DB, TBeacon, V>
where
    TBeacon: Beacon + Sync + Send + 'static,
    DB: BlockStore + Sync + Send + 'static,
    V: ProofVerifier + Sync + Send + Unpin + 'static,
{
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        tracker: crate::chain_muxer::WorkerState,
        proposed_head: Arc<Tipset>,
        current_head: Arc<Tipset>,
        state_manager: Arc<StateManager<DB>>,
        beacon: Arc<BeaconSchedule<TBeacon>>,
        network: SyncNetworkContext<DB>,
        chain_store: Arc<ChainStore<DB>>,
        bad_block_cache: Arc<BadBlockCache>,
        genesis: Arc<Tipset>,
    ) -> Result<Self, TipsetRangeSyncerError> {
        let tipset_tasks = Box::pin(FuturesUnordered::new());
        let tipset_range_length = proposed_head.epoch() - current_head.epoch();

        // Ensure the difference in epochs between the proposed and current head is >= 0
        if tipset_range_length < 0 {
            return Err(TipsetRangeSyncerError::InvalidTipsetRangeLength);
        }

        tipset_tasks.push(sync_tipset_range::<_, _, V>(
            proposed_head.clone(),
            current_head.clone(),
            tracker,
            // Casting from i64 -> u64 is safe because we ensured that
            // the value is greater than 0
            tipset_range_length as u64,
            state_manager.clone(),
            chain_store.clone(),
            network.clone(),
            bad_block_cache.clone(),
            beacon.clone(),
            genesis.clone(),
        ));

        let mut tipsets_included = HashSet::new();
        tipsets_included.insert(proposed_head.key());
        Ok(Self {
            proposed_head,
            current_head,
            tipsets_included: HashSet::new(),
            tipset_tasks,
            state_manager,
            beacon,
            network,
            chain_store,
            bad_block_cache,
            genesis,
            verifier: Default::default(),
        })
    }

    pub fn add_tipset(
        &mut self,
        additional_head: Arc<Tipset>,
    ) -> Result<bool, TipsetRangeSyncerError> {
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

        self.tipset_tasks.push(sync_tipset::<_, _, V>(
            additional_head,
            self.state_manager.clone(),
            self.chain_store.clone(),
            self.network.clone(),
            self.bad_block_cache.clone(),
            self.beacon.clone(),
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

impl<DB, TBeacon, V> Future for TipsetRangeSyncer<DB, TBeacon, V>
where
    TBeacon: Beacon + Sync + Send + 'static,
    DB: BlockStore + Sync + Send + 'static,
    V: Sync + Send + Unpin + 'static,
{
    type Output = Result<(), TipsetRangeSyncerError>;

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

#[allow(clippy::too_many_arguments)]
fn sync_tipset_range<
    DB: BlockStore + Sync + Send + 'static,
    TBeacon: Beacon + Sync + Send + 'static,
    V: ProofVerifier + Sync + Send + 'static,
>(
    proposed_head: Arc<Tipset>,
    current_head: Arc<Tipset>,
    tracker: crate::chain_muxer::WorkerState,
    tipset_range_length: u64,
    state_manager: Arc<StateManager<DB>>,
    chain_store: Arc<ChainStore<DB>>,
    network: SyncNetworkContext<DB>,
    bad_block_cache: Arc<BadBlockCache>,
    beacon: Arc<BeaconSchedule<TBeacon>>,
    genesis: Arc<Tipset>,
) -> TipsetRangeSyncerFuture {
    Box::pin(async move {
        tracker
            .write()
            .await
            .init(current_head.clone(), proposed_head.clone());

        let parent_tipsets = match sync_headers_in_reverse(
            tracker.clone(),
            tipset_range_length,
            proposed_head.clone(),
            current_head.clone(),
            bad_block_cache.clone(),
            chain_store.clone(),
            network.clone(),
        )
        .await
        {
            Ok(parent_tipsets) => parent_tipsets,
            Err(why) => {
                tracker.write().await.error(why.to_string());
                return Err(why);
            }
        };

        // Persist the blocks from the synced Tipsets into the store
        tracker.write().await.set_stage(SyncStage::Headers);
        let headers: Vec<&BlockHeader> = parent_tipsets.iter().flat_map(|t| t.blocks()).collect();
        if let Err(why) = persist_objects(chain_store.blockstore(), &headers) {
            tracker.write().await.error(why.to_string());
            return Err(why.into());
        };

        //  Sync and validate messages from the tipsets
        tracker.write().await.set_stage(SyncStage::Messages);
        if let Err(why) = sync_messages_check_state::<_, _, V>(
            tracker.clone(),
            state_manager,
            beacon,
            network,
            chain_store.clone(),
            bad_block_cache,
            parent_tipsets,
            genesis,
            InvalidBlockStrategy::Strict,
        )
        .await
        {
            error!("Sync messages check state failed for tipset range");
            tracker.write().await.error(why.to_string());
            return Err(why);
        };
        tracker.write().await.set_stage(SyncStage::Complete);

        // At this point the head is synced and it can be set in the store as the heaviest
        debug!(
            "Tipset range successfully verified: EPOCH = [{}, {}], HEAD_KEY = {:?}",
            proposed_head.epoch(),
            current_head.epoch(),
            proposed_head.key()
        );
        if let Err(why) = chain_store.put_tipset(&proposed_head).await {
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

async fn sync_headers_in_reverse<DB: BlockStore + Sync + Send + 'static>(
    tracker: crate::chain_muxer::WorkerState,
    tipset_range_length: u64,
    proposed_head: Arc<Tipset>,
    current_head: Arc<Tipset>,
    bad_block_cache: Arc<BadBlockCache>,
    chain_store: Arc<ChainStore<DB>>,
    network: SyncNetworkContext<DB>,
) -> Result<Vec<Arc<Tipset>>, TipsetRangeSyncerError> {
    let mut parent_blocks: Vec<Cid> = vec![];
    let mut parent_tipsets = Vec::with_capacity(tipset_range_length as usize + 1);
    parent_tipsets.push(proposed_head.clone());
    tracker.write().await.set_epoch(current_head.epoch());

    'sync: loop {
        // Unwrapping is safe here because the tipset vector always
        // has at least one element
        let oldest_parent = parent_tipsets.last().unwrap();
        validate_tipset_against_cache(
            bad_block_cache.clone(),
            oldest_parent.parents(),
            &parent_blocks,
        )
        .await?;

        // Check if we are at the end of the range
        if oldest_parent.epoch() <= current_head.epoch() {
            // Current tipset epoch is less than or equal to the epoch of
            // Tipset we a synchronizing toward, stop.
            break;
        }
        // Attempt to load the parent tipset from local store
        if let Ok(tipset) = chain_store.tipset_from_keys(oldest_parent.parents()).await {
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
            validate_tipset_against_cache(bad_block_cache.clone(), &tipset.key(), &parent_blocks)
                .await?;
            parent_blocks.extend_from_slice(tipset.cids());
            tracker.write().await.set_epoch(tipset.epoch());
            parent_tipsets.push(tipset);
        }
    }
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
        let mut potential_common_ancestor =
            chain_store.tipset_from_keys(current_head.parents()).await?;
        let mut fork_length = 1;
        for (i, tipset) in fork_tipsets.iter().enumerate() {
            if tipset.epoch() == 0 {
                return Err(TipsetRangeSyncerError::ForkAtGenesisBlock(format!(
                    "{:?}",
                    oldest_tipset.cids()
                )));
            }
            if potential_common_ancestor == *tipset {
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
            if potential_common_ancestor.epoch() < tipset.epoch() {
                continue;
            }
            fork_length += 1;
            // Increment the fork length and enfore the fork length check
            if fork_length > FORK_LENGTH_THRESHOLD {
                return Err(TipsetRangeSyncerError::ChainForkLengthExceedsMaximum);
            }
            // If we have not found a common ancestor by the last iteration, then return an error
            if i == (fork_tipsets.len() - 1) {
                return Err(TipsetRangeSyncerError::ChainForkLengthExceedsFinalityThreshold);
            }
            potential_common_ancestor = chain_store
                .tipset_from_keys(potential_common_ancestor.parents())
                .await?;
        }
    }
    Ok(parent_tipsets)
}

#[allow(clippy::too_many_arguments)]
fn sync_tipset<
    DB: BlockStore + Sync + Send + 'static,
    TBeacon: Beacon + Sync + Send + 'static,
    V: ProofVerifier + Sync + Send + 'static,
>(
    proposed_head: Arc<Tipset>,
    state_manager: Arc<StateManager<DB>>,
    chain_store: Arc<ChainStore<DB>>,
    network: SyncNetworkContext<DB>,
    bad_block_cache: Arc<BadBlockCache>,
    beacon: Arc<BeaconSchedule<TBeacon>>,
    genesis: Arc<Tipset>,
) -> TipsetRangeSyncerFuture {
    Box::pin(async move {
        // Persist the blocks from the proposed tipsets into the store
        let headers: Vec<&BlockHeader> = proposed_head.blocks().iter().collect();
        persist_objects(chain_store.blockstore(), &headers)?;

        // Sync and validate messages from the tipsets
        if let Err(e) = sync_messages_check_state::<_, _, V>(
            // Include a dummy WorkerState
            crate::chain_muxer::WorkerState::default(),
            state_manager,
            beacon,
            network,
            chain_store.clone(),
            bad_block_cache,
            vec![proposed_head.clone()],
            genesis,
            InvalidBlockStrategy::Forgiving,
        )
        .await
        {
            error!("Sync messages check state failed for single tipset");
            return Err(e);
        }

        // Add the tipset to the store. The tipset will be expanded with other blocks with
        // the same [epoch, parents] before updating the heaviest Tipset in the store.
        if let Err(why) = chain_store.put_tipset(&proposed_head).await {
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

#[allow(clippy::too_many_arguments)]
async fn sync_messages_check_state<
    DB: BlockStore + Send + Sync + 'static,
    TBeacon: Beacon + Sync + Send + 'static,
    V: ProofVerifier + Sync + Send + 'static,
>(
    tracker: crate::chain_muxer::WorkerState,
    state_manager: Arc<StateManager<DB>>,
    beacon_scheduler: Arc<BeaconSchedule<TBeacon>>,
    network: SyncNetworkContext<DB>,
    chainstore: Arc<ChainStore<DB>>,
    bad_block_cache: Arc<BadBlockCache>,
    tipsets: Vec<Arc<Tipset>>,
    genesis: Arc<Tipset>,
    invalid_block_strategy: InvalidBlockStrategy,
) -> Result<(), TipsetRangeSyncerError> {
    // Iterate through tipsets in chronological order
    let mut tipset_iter = tipsets.into_iter().rev();

    // Sync the messages for one tipset @ a time
    const REQUEST_WINDOW: usize = 1;

    while let Some(tipset) = tipset_iter.next() {
        match chainstore.fill_tipset(&tipset) {
            Some(full_tipset) => {
                let current_epoch = full_tipset.epoch();
                validate_tipset::<_, _, V>(
                    state_manager.clone(),
                    beacon_scheduler.clone(),
                    chainstore.clone(),
                    bad_block_cache.clone(),
                    full_tipset,
                    genesis.clone(),
                    invalid_block_strategy,
                )
                .await?;
                tracker.write().await.set_epoch(current_epoch);
            }
            None => {
                // Full tipset is not in storage; request messages via chain_exchange
                let batch_size = REQUEST_WINDOW;
                debug!(
                    "ChainExchange message sync tipsets: epoch: {}, len: {}",
                    tipset.epoch(),
                    batch_size,
                );

                // Receive tipset bundle from block sync
                let compacted_messages = network
                    .chain_exchange_messages(None, tipset.key(), batch_size as u64)
                    .await
                    .map_err(TipsetRangeSyncerError::NetworkMessageQueryFailed)?;
                // Chain current tipset with iterator
                let mut inner_iter = std::iter::once(tipset).chain(&mut tipset_iter);

                // Since the bundle only has messages, we have to put the headers in them
                for messages in compacted_messages {
                    // Construct full tipset from fetched messages
                    let tipset = inner_iter.next().ok_or_else(|| {
                        TipsetRangeSyncerError::NetworkMessageQueryFailed(String::from(
                            "Messages returned exceeded tipsets in chain",
                        ))
                    })?;

                    let bundle = TipsetBundle {
                        blocks: tipset.blocks().to_vec(),
                        messages: Some(messages),
                    };

                    let full_tipset = FullTipset::try_from(&bundle)
                        .map_err(TipsetRangeSyncerError::GeneratingTipsetFromTipsetBundle)?;

                    // Validate the tipset and the messages
                    let timer = metrics::TIPSET_PROCESSING_TIME.start_timer();
                    let current_epoch = full_tipset.epoch();
                    validate_tipset::<_, _, V>(
                        state_manager.clone(),
                        beacon_scheduler.clone(),
                        chainstore.clone(),
                        bad_block_cache.clone(),
                        full_tipset,
                        genesis.clone(),
                        invalid_block_strategy,
                    )
                    .await?;
                    tracker.write().await.set_epoch(current_epoch);
                    timer.observe_duration();

                    // Persist the messages in the store
                    if let Some(m) = bundle.messages {
                        chain::persist_objects(chainstore.blockstore(), &m.bls_msgs)?;
                        chain::persist_objects(chainstore.blockstore(), &m.secp_msgs)?;
                    } else {
                        warn!("ChainExchange request for messages returned null messages");
                    }
                }
            }
        }
    }
    Ok(())
}

async fn validate_tipset<
    DB: BlockStore + Send + Sync + 'static,
    TBeacon: Beacon + Sync + Send + 'static,
    V: ProofVerifier + Sync + Send + 'static,
>(
    state_manager: Arc<StateManager<DB>>,
    beacon_scheduler: Arc<BeaconSchedule<TBeacon>>,
    chainstore: Arc<ChainStore<DB>>,
    bad_block_cache: Arc<BadBlockCache>,
    full_tipset: FullTipset,
    genesis: Arc<Tipset>,
    invalid_block_strategy: InvalidBlockStrategy,
) -> Result<(), TipsetRangeSyncerError> {
    if full_tipset.key().eq(genesis.key()) {
        trace!("Skipping genesis tipset validation");
        return Ok(());
    }

    let epoch = full_tipset.epoch();
    let full_tipset_key = full_tipset.key().clone();

    let mut validations = FuturesUnordered::new();
    for b in full_tipset.into_blocks() {
        let validation_fn = task::spawn(validate_block::<_, _, V>(
            state_manager.clone(),
            beacon_scheduler.clone(),
            Arc::new(b),
        ));
        validations.push(validation_fn);
    }

    while let Some(result) = validations.next().await {
        match result {
            Ok(block) => {
                chainstore.add_to_tipset_tracker(block.header()).await;
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
                            bad_block_cache.put(cid, why.to_string()).await;
                        }
                    }
                }
                return Err(why);
            }
        }
    }
    info!(
        "Validating tipset: EPOCH = {}, KEY = {:?}",
        epoch, full_tipset_key.cids,
    );
    Ok(())
}

/// Validates block semantically according to https://github.com/filecoin-project/specs/blob/6ab401c0b92efb6420c6e198ec387cf56dc86057/validation.md
/// Returns the validated block if `Ok`.
/// Returns the block cid (for marking bad) and `Error` if invalid (`Err`).
async fn validate_block<
    DB: BlockStore + Sync + Send + 'static,
    TBeacon: Beacon + Sync + Send + 'static,
    V: ProofVerifier + Sync + Send + 'static,
>(
    state_manager: Arc<StateManager<DB>>,
    beacon_schedule: Arc<BeaconSchedule<TBeacon>>,
    block: Arc<Block>,
) -> Result<Arc<Block>, (Cid, TipsetRangeSyncerError)> {
    trace!(
        "Validating block: epoch = {}, weight = {}, key = {}",
        block.header().epoch(),
        block.header().weight(),
        block.header().cid(),
    );
    let chain_store = state_manager.chain_store().clone();
    let block_cid = block.cid();

    // Check block validation cache in store
    let is_validated = chain_store
        .is_block_validated(block_cid)
        .map_err(|why| (*block_cid, why.into()))?;
    if is_validated {
        return Ok(block);
    }
    let mut error_vec: Vec<String> = vec![];
    let mut validations = FuturesUnordered::new();
    let header = block.header();

    // Check to ensure all optional values exist
    block_sanity_checks(&header).map_err(|e| (*block_cid, e))?;

    let base_tipset = chain_store
        .tipset_from_keys(header.parents())
        .await
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
    let win_p_nv = state_manager.get_network_version(base_tipset.epoch());

    // Retrieve lookback tipset for validation
    let (lookback_tipset, lookback_state) = state_manager
        .get_lookback_tipset_for_round::<V>(base_tipset.clone(), block.header().epoch())
        .await
        .map_err(|e| (*block_cid, e.into()))?;
    let lookback_state = Arc::new(lookback_state);
    let prev_beacon = chain_store
        .latest_beacon_entry(&base_tipset)
        .await
        .map(Arc::new)
        .map_err(|e| (*block_cid, e.into()))?;

    // Timestamp checks
    let nulls = (header.epoch() - (base_tipset.epoch() + 1)) as u64;
    let target_timestamp = base_tipset.min_timestamp() + BLOCK_DELAY_SECS * (nulls + 1);
    if target_timestamp != header.timestamp() {
        return Err((
            *block_cid,
            TipsetRangeSyncerError::UnequalBlockTimestamps(header.timestamp(), target_timestamp),
        ));
    }
    let time_now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Retrieved system time before UNIX epoch")
        .as_secs();
    if header.timestamp() > time_now + ALLOWABLE_CLOCK_DRIFT {
        return Err((
            *block_cid,
            TipsetRangeSyncerError::TimeTravellingBlock(time_now, header.timestamp()),
        ));
    } else if header.timestamp() > time_now {
        warn!(
            "Got block from the future, but within clock drift threshold, {} > {}",
            header.timestamp(),
            time_now
        );
    }

    // Work address needed for async validations, so necessary
    // to do sync to avoid duplication
    let work_addr = state_manager
        .get_miner_work_addr(&lookback_state, header.miner_address())
        .map_err(|e| (*block_cid, e.into()))?;

    // Async validations

    // Check block messages
    let v_block = Arc::clone(&block);
    let v_base_tipset = Arc::clone(&base_tipset);
    let v_state_manager = Arc::clone(&state_manager);
    validations.push(task::spawn_blocking(move || {
        check_block_messages::<_, V>(v_state_manager, &v_block, &v_base_tipset)
            .map_err(|e| TipsetRangeSyncerError::Validation(e.to_string()))
    }));

    // Miner validations
    let v_state_manager = Arc::clone(&state_manager);
    let v_block = Arc::clone(&block);
    let v_base_tipset = Arc::clone(&base_tipset);
    validations.push(task::spawn_blocking(move || {
        let headers = v_block.header();
        validate_miner(
            &v_state_manager,
            headers.miner_address(),
            v_base_tipset.parent_state(),
        )
    }));

    // Base fee check
    let v_base_tipset = Arc::clone(&base_tipset);
    let v_block_store = state_manager.blockstore_cloned();
    let v_block = Arc::clone(&block);
    validations.push(task::spawn_blocking(move || {
        let base_fee =
            chain::compute_base_fee(v_block_store.as_ref(), &v_base_tipset).map_err(|e| {
                TipsetRangeSyncerError::Validation(format!(
                    "Could not compute base fee: {}",
                    e.to_string()
                ))
            })?;
        let parent_base_fee = v_block.header.parent_base_fee();
        if &base_fee != parent_base_fee {
            return Err(TipsetRangeSyncerError::Validation(format!(
                "base fee doesn't match: {} (header), {} (computed)",
                parent_base_fee, base_fee
            )));
        }
        Ok(())
    }));

    // Parent weight calculation check
    let v_block_store = state_manager.blockstore_cloned();
    let v_base_tipset = Arc::clone(&base_tipset);
    let weight = header.weight().clone();
    validations.push(task::spawn_blocking(move || {
        let calc_weight = chain::weight(v_block_store.as_ref(), &v_base_tipset).map_err(|e| {
            TipsetRangeSyncerError::Calculation(format!("Error calculating weight: {}", e))
        })?;
        if weight != calc_weight {
            return Err(TipsetRangeSyncerError::Validation(format!(
                "Parent weight doesn't match: {} (header), {} (computed)",
                weight, calc_weight
            )));
        }
        Ok(())
    }));

    // State root and receipt root validations
    let v_state_manager = Arc::clone(&state_manager);
    let v_base_tipset = Arc::clone(&base_tipset);
    let v_block = Arc::clone(&block);
    validations.push(task::spawn(async move {
        let header = v_block.header();
        let (state_root, receipt_root) = v_state_manager
            .tipset_state::<V>(&v_base_tipset)
            .await
            .map_err(|e| {
                TipsetRangeSyncerError::Calculation(format!("Failed to calculate state: {}", e))
            })?;
        if &state_root != header.state_root() {
            return Err(TipsetRangeSyncerError::Validation(format!(
                "Parent state root did not match computed state: {} (header), {} (computed)",
                header.state_root(),
                state_root,
            )));
        }
        if &receipt_root != header.message_receipts() {
            return Err(TipsetRangeSyncerError::Validation(format!(
                "Parent receipt root did not match computed root: {} (header), {} (computed)",
                header.message_receipts(),
                receipt_root
            )));
        }
        Ok(())
    }));

    // Winner election PoSt validations
    let v_block = Arc::clone(&block);
    let v_prev_beacon = Arc::clone(&prev_beacon);
    let v_base_tipset = Arc::clone(&base_tipset);
    let v_state_manager = Arc::clone(&state_manager);
    let v_lookback_state = lookback_state.clone();
    validations.push(task::spawn_blocking(move || {
        let header = v_block.header();

        // Safe to unwrap because checked to `Some` in sanity check
        let election_proof = header.election_proof().as_ref().unwrap();
        if election_proof.win_count < 1 {
            return Err(TipsetRangeSyncerError::Validation(
                "Block is not claiming to be a winner".to_string(),
            ));
        }
        let hp = v_state_manager.eligible_to_mine(
            header.miner_address(),
            &v_base_tipset,
            &lookback_tipset,
        )?;
        if !hp {
            return Err(TipsetRangeSyncerError::MinerNotEligibleToMine);
        }
        let r_beacon = header.beacon_entries().last().unwrap_or(&v_prev_beacon);
        let miner_address_buf = header.miner_address().marshal_cbor()?;
        let vrf_base = chain::draw_randomness(
            r_beacon.data(),
            DomainSeparationTag::ElectionProofProduction,
            header.epoch(),
            &miner_address_buf,
        )
        .map_err(|e| TipsetRangeSyncerError::DrawingChainRandomness(e.to_string()))?;
        verify_election_post_vrf(&work_addr, &vrf_base, election_proof.vrfproof.as_bytes())?;

        if v_state_manager
            .is_miner_slashed(header.miner_address(), &v_base_tipset.parent_state())?
        {
            return Err(TipsetRangeSyncerError::InvalidOrSlashedMiner);
        }
        let (mpow, tpow) = v_state_manager
            .get_power(&v_lookback_state, Some(header.miner_address()))?
            .ok_or(TipsetRangeSyncerError::MinerPowerNotAvailable)?;

        let j = election_proof.compute_win_count(&mpow.quality_adj_power, &tpow.quality_adj_power);
        if election_proof.win_count != j {
            return Err(TipsetRangeSyncerError::MinerWinClaimsIncorrect(
                election_proof.win_count,
                j,
            ));
        }

        Ok(())
    }));

    // Block signature check
    let v_block = Arc::clone(&block);
    validations.push(task::spawn_blocking(move || {
        v_block.header().check_block_signature(&work_addr)?;
        Ok(())
    }));

    // Beacon values check
    if std::env::var(IGNORE_DRAND_VAR) != Ok("1".to_owned()) {
        let v_block = Arc::clone(&block);
        let parent_epoch = base_tipset.epoch();
        let v_prev_beacon = Arc::clone(&prev_beacon);
        validations.push(task::spawn(async move {
            v_block
                .header()
                .validate_block_drand(beacon_schedule.as_ref(), parent_epoch, &v_prev_beacon)
                .await
                .map_err(|e| {
                    TipsetRangeSyncerError::Validation(format!(
                        "Failed to validate blocks random beacon values: {}",
                        e
                    ))
                })
        }));
    }

    // Ticket election proof validations
    let v_block = Arc::clone(&block);
    let v_prev_beacon = Arc::clone(&prev_beacon);
    validations.push(task::spawn_blocking(move || {
        let header = v_block.header();
        let mut miner_address_buf = header.miner_address().marshal_cbor()?;

        if header.epoch() > UPGRADE_SMOKE_HEIGHT {
            let vrf_proof = base_tipset
                .min_ticket()
                .ok_or(TipsetRangeSyncerError::TipsetWithoutTicket)?
                .vrfproof
                .as_bytes();
            miner_address_buf.extend_from_slice(vrf_proof);
        }

        let beacon_base = header.beacon_entries().last().unwrap_or(&v_prev_beacon);

        let vrf_base = chain::draw_randomness(
            beacon_base.data(),
            DomainSeparationTag::TicketProduction,
            header.epoch() - TICKET_RANDOMNESS_LOOKBACK,
            &miner_address_buf,
        )
        .map_err(|e| TipsetRangeSyncerError::DrawingChainRandomness(e.to_string()))?;

        verify_election_post_vrf(
            &work_addr,
            &vrf_base,
            // Safe to unwrap here because of block sanity checks
            header.ticket().as_ref().unwrap().vrfproof.as_bytes(),
        )?;

        Ok(())
    }));

    // Winning PoSt proof validation
    let v_block = block.clone();
    let v_prev_beacon = Arc::clone(&prev_beacon);
    validations.push(task::spawn_blocking(move || {
        verify_winning_post_proof::<_, V>(
            &state_manager,
            win_p_nv,
            v_block.header(),
            &v_prev_beacon,
            &lookback_state,
        )?;
        Ok(())
    }));

    // Collect the errors from the async validations
    while let Some(result) = validations.next().await {
        if let Err(e) = result {
            error_vec.push(e.to_string());
        }
    }

    // Combine the vector of error strings and return Validation error with this resultant string
    if !error_vec.is_empty() {
        let error_string = error_vec.join(", ");
        return Err((*block_cid, TipsetRangeSyncerError::Validation(error_string)));
    }

    chain_store
        .mark_block_as_validated(block_cid)
        .map_err(|e| {
            (
                *block_cid,
                TipsetRangeSyncerError::Validation(format!(
                    "failed to mark block {} as validated {}",
                    block_cid, e
                )),
            )
        })?;

    Ok(block)
}

fn validate_miner<DB: BlockStore + Send + Sync + 'static>(
    state_manager: &StateManager<DB>,
    miner_addr: &Address,
    tipset_state: &Cid,
) -> Result<(), TipsetRangeSyncerError> {
    let actor = state_manager
        .get_actor(power::ADDRESS, tipset_state)?
        .ok_or(TipsetRangeSyncerError::PowerActorUnavailable)?;
    let state = power::State::load(state_manager.blockstore(), &actor)
        .map_err(|err| TipsetRangeSyncerError::MinerPowerUnavailable(err.to_string()))?;
    state
        .miner_power(state_manager.blockstore(), miner_addr)
        .map_err(|err| TipsetRangeSyncerError::MinerPowerUnavailable(err.to_string()))?;
    Ok(())
}

fn verify_election_post_vrf(
    worker: &Address,
    rand: &[u8],
    evrf: &[u8],
) -> Result<(), TipsetRangeSyncerError> {
    crypto::verify_vrf(worker, rand, evrf).map_err(TipsetRangeSyncerError::VrfValidation)
}

fn verify_winning_post_proof<DB: BlockStore + Send + Sync + 'static, V: ProofVerifier>(
    state_manager: &StateManager<DB>,
    network_version: NetworkVersion,
    header: &BlockHeader,
    prev_beacon_entry: &BeaconEntry,
    lookback_state: &Cid,
) -> Result<(), TipsetRangeSyncerError> {
    if cfg!(feature = "insecure_post") {
        let wpp = header.winning_post_proof();
        if wpp.is_empty() {
            return Err(TipsetRangeSyncerError::InsecurePostValidation(
                String::from("No winning PoSt proof provided"),
            ));
        }
        if wpp[0].proof_bytes == b"valid_proof" {
            return Ok(());
        }
        return Err(TipsetRangeSyncerError::InsecurePostValidation(
            String::from("Winning PoSt is invalid"),
        ));
    }

    let miner_addr_buf = header.miner_address().marshal_cbor()?;
    let rand_base = header
        .beacon_entries()
        .iter()
        .last()
        .unwrap_or(prev_beacon_entry);
    let rand = chain::draw_randomness(
        rand_base.data(),
        DomainSeparationTag::WinningPoStChallengeSeed,
        header.epoch(),
        &miner_addr_buf,
    )
    .map_err(|e| TipsetRangeSyncerError::DrawingChainRandomness(e.to_string()))?;
    let id = header.miner_address().id().map_err(|e| {
        TipsetRangeSyncerError::Validation(format!(
            "failed to get ID from miner address {}: {}",
            header.miner_address(),
            e
        ))
    })?;
    let sectors = state_manager
        .get_sectors_for_winning_post::<V>(
            &lookback_state,
            network_version,
            &header.miner_address(),
            Randomness(rand.to_vec()),
        )
        .map_err(|e| {
            TipsetRangeSyncerError::Validation(format!(
                "Failed to get sectors for PoSt: {}",
                e.to_string()
            ))
        })?;

    V::verify_winning_post(
        Randomness(rand.to_vec()),
        header.winning_post_proof(),
        &sectors,
        id,
    )
    .map_err(|e| {
        TipsetRangeSyncerError::Validation(format!(
            "Failed to verify winning PoSt: {}",
            e.to_string()
        ))
    })
}

fn check_block_messages<
    DB: BlockStore + Send + Sync + 'static,
    V: ProofVerifier + Sync + Send + 'static,
>(
    state_manager: Arc<StateManager<DB>>,
    block: &Block,
    base_tipset: &Arc<Tipset>,
) -> Result<(), TipsetRangeSyncerError> {
    let network_version = get_network_version_default(block.header.epoch());

    // Do the initial loop here
    // check block message and signatures in them
    let mut pub_keys = Vec::new();
    let mut cids = Vec::new();
    for m in block.bls_msgs() {
        let pk = StateManager::get_bls_public_key(
            state_manager.blockstore(),
            m.from(),
            base_tipset.parent_state(),
        )?;
        pub_keys.push(pk);
        cids.push(m.to_signing_bytes());
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
            &sig,
        ) {
            return Err(TipsetRangeSyncerError::BlsAggregateSignatureInvalid(
                format!("{:?}", sig),
                format!("{:?}", cids),
            ));
        }
    } else {
        return Err(TipsetRangeSyncerError::BlockWithoutBlsAggregate);
    }
    let price_list = price_list_by_epoch(base_tipset.epoch());
    let mut sum_gas_limit = 0;

    // Check messages for validity
    let mut check_msg = |msg: &UnsignedMessage,
                         account_sequences: &mut HashMap<Address, u64>,
                         tree: &StateTree<DB>|
     -> Result<(), Box<dyn StdError>> {
        // Phase 1: Syntactic validation
        let min_gas = price_list.on_chain_message(msg.marshal_cbor().unwrap().len());
        msg.valid_for_block_inclusion(min_gas.total(), network_version)?;
        sum_gas_limit += msg.gas_limit();
        if sum_gas_limit > BLOCK_GAS_LIMIT {
            return Err("block gas limit exceeded".into());
        }

        // Phase 2: (Partial) Semantic validation
        // Send exists and is an account actor, and sequence is correct
        let sequence: u64 = match account_sequences.get(msg.from()) {
            Some(sequence) => *sequence,
            None => {
                let actor = tree.get_actor(msg.from())?.ok_or({
                    "Failed to retrieve nonce for addr: Actor does not exist in state"
                })?;
                if !is_account_actor(&actor.code) {
                    return Err("Sending must be an account actor".into());
                }
                actor.sequence
            }
        };

        // Sequence equality check
        if sequence != msg.sequence() {
            return Err(format!(
                "Message has incorrect sequence (exp: {} got: {})",
                sequence,
                msg.sequence()
            )
            .into());
        }
        account_sequences.insert(*msg.from(), sequence + 1);
        Ok(())
    };

    let mut account_sequences: HashMap<Address, u64> = HashMap::default();
    let block_store = state_manager.blockstore();
    let (state_root, _) =
        task::block_on(state_manager.tipset_state::<V>(&base_tipset)).map_err(|e| {
            TipsetRangeSyncerError::Calculation(format!("Could not update state: {}", e))
        })?;
    let tree = StateTree::new_from_root(block_store, &state_root).map_err(|e| {
        TipsetRangeSyncerError::Calculation(format!(
            "Could not load from new state root in state manager: {}",
            e
        ))
    })?;

    // Check validity for BLS messages
    for (i, msg) in block.bls_msgs().iter().enumerate() {
        check_msg(msg, &mut account_sequences, &tree).map_err(|e| {
            TipsetRangeSyncerError::Validation(format!(
                "Block had invalid BLS message at index {}: {}",
                i, e
            ))
        })?;
    }

    // Check validity for SECP messages
    for (i, msg) in block.secp_msgs().iter().enumerate() {
        check_msg(msg.message(), &mut account_sequences, &tree).map_err(|e| {
            TipsetRangeSyncerError::Validation(format!(
                "block had an invalid secp message at index {}: {}",
                i, e
            ))
        })?;
        // Resolve key address for signature verification
        let key_addr =
            task::block_on(state_manager.resolve_to_key_addr::<V>(msg.from(), base_tipset))
                .map_err(|e| TipsetRangeSyncerError::ResolvingAddressFromMessage(e.to_string()))?;
        // SecP256K1 Signature validation
        msg.signature
            .verify(&msg.message().to_signing_bytes(), &key_addr)
            .map_err(TipsetRangeSyncerError::MessageSignatureInvalid)?;
    }

    // Validate message root from header matches message root
    let msg_root =
        TipsetValidator::compute_msg_root(block_store, block.bls_msgs(), block.secp_msgs())
            .map_err(|err| TipsetRangeSyncerError::ComputingMessageRoot(err.to_string()))?;
    if block.header().messages() != &msg_root {
        return Err(TipsetRangeSyncerError::BlockMessageRootInvalid(
            format!("{:?}", block.header().messages()),
            format!("{:?}", msg_root),
        ));
    }

    Ok(())
}

/// Checks optional values in header and returns reference to the values.
fn block_sanity_checks(header: &BlockHeader) -> Result<(), TipsetRangeSyncerError> {
    if header.election_proof().is_none() {
        return Err(TipsetRangeSyncerError::BlockWithoutElectionProof);
    }
    if header.signature().is_none() {
        return Err(TipsetRangeSyncerError::BlockWithoutSignature);
    }
    if header.bls_aggregate().is_none() {
        return Err(TipsetRangeSyncerError::BlockWithoutBlsAggregate);
    }
    if header.ticket().is_none() {
        return Err(TipsetRangeSyncerError::BlockWithoutTicket);
    }
    Ok(())
}

async fn validate_tipset_against_cache(
    bad_block_cache: Arc<BadBlockCache>,
    tipset: &TipsetKeys,
    descendant_blocks: &[Cid],
) -> Result<(), TipsetRangeSyncerError> {
    for cid in tipset.cids() {
        if let Some(reason) = bad_block_cache.get(cid).await {
            for block_cid in descendant_blocks {
                bad_block_cache
                    .put(*block_cid, format!("chain contained {}", cid))
                    .await;
            }
            return Err(TipsetRangeSyncerError::TipsetRangeWithBadBlock(
                *cid, reason,
            ));
        }
    }
    Ok(())
}
