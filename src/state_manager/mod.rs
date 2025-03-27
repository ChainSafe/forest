// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod chain_rand;
pub mod circulating_supply;
mod errors;
pub mod utils;
pub use self::errors::*;
use self::utils::structured;

use crate::beacon::{BeaconEntry, BeaconSchedule};
use crate::blocks::{Tipset, TipsetKey};
use crate::chain::{
    index::{ChainIndex, ResolveNullTipset},
    ChainStore, HeadChange,
};
use crate::interpreter::{
    resolve_to_key_addr, ApplyResult, BlockMessages, CalledAt, ExecutionContext, VMEvent,
    IMPLICIT_MESSAGE_GAS_LIMIT, VM,
};
use crate::interpreter::{MessageCallbackCtx, VMTrace};
use crate::lotus_json::{lotus_json_with_self, LotusJson};
use crate::message::{ChainMessage, Message as MessageTrait};
use crate::networks::ChainConfig;
use crate::rpc::state::{ApiInvocResult, InvocResult, MessageGasCost};
use crate::rpc::types::{MiningBaseInfo, SectorOnChainInfo};
use crate::shim::actors::init::{self, State};
use crate::shim::actors::miner::{MinerInfo, MinerPower, Partition};
use crate::shim::actors::verifreg::{Allocation, AllocationID, Claim};
use crate::shim::actors::*;
use crate::shim::{
    actors::{
        miner::ext::MinerStateExt as _, verifreg::ext::VerifiedRegistryStateExt as _,
        LoadActorStateFromBlockstore,
    },
    executor::{ApplyRet, Receipt, StampedEvent},
};
use crate::shim::{
    address::{Address, Payload, Protocol},
    clock::ChainEpoch,
    econ::TokenAmount,
    message::Message,
    randomness::Randomness,
    state_tree::{ActorState, StateTree},
    version::NetworkVersion,
};
use crate::state_manager::chain_rand::draw_randomness;
use crate::state_migration::run_state_migrations;
use ahash::{HashMap, HashMapExt};
use anyhow::{bail, Context as _};
use bls_signatures::{PublicKey as BlsPublicKey, Serialize as _};
use chain_rand::ChainRand;
use cid::Cid;
pub use circulating_supply::GenesisInfo;
use fil_actor_verifreg_state::v12::DataCap;
use fil_actor_verifreg_state::v13::ClaimID;
use fil_actors_shared::fvm_ipld_amt::Amtv0 as Amt;
use fil_actors_shared::fvm_ipld_bitfield::BitField;
use fil_actors_shared::v12::runtime::DomainSeparationTag;
use fil_actors_shared::v13::runtime::Policy;
use futures::{channel::oneshot, select, FutureExt};
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::to_vec;
use itertools::Itertools as _;
use lru::LruCache;
use nonzero_ext::nonzero;
use num::BigInt;
use num_traits::identities::Zero;
use parking_lot::Mutex as SyncMutex;
use rayon::prelude::ParallelBridge;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::ops::RangeInclusive;
use std::{num::NonZeroUsize, sync::Arc};
use tokio::sync::{broadcast::error::RecvError, Mutex as TokioMutex, RwLock};
use tracing::{error, info, instrument, trace, warn};
pub use utils::is_valid_for_sending;

const DEFAULT_TIPSET_CACHE_SIZE: NonZeroUsize = nonzero!(1024usize);

const DEFAULT_EVENT_CACHE_SIZE: NonZeroUsize = nonzero!(4096usize);

/// Intermediary for retrieving state objects and updating actor states.
type CidPair = (Cid, Cid);

#[derive(Clone)]
pub struct StateOutput {
    pub state_root: Cid,
    pub receipt_root: Cid,
    pub events: Vec<Vec<StampedEvent>>,
}

#[derive(Clone)]
pub struct StateOutputValue {
    pub state_root: Cid,
    pub receipt_root: Cid,
}

impl From<StateOutputValue> for StateOutput {
    fn from(value: StateOutputValue) -> Self {
        Self {
            state_root: value.state_root,
            receipt_root: value.receipt_root,
            events: vec![],
        }
    }
}

impl From<StateOutput> for StateOutputValue {
    fn from(value: StateOutput) -> Self {
        StateOutputValue {
            state_root: value.state_root,
            receipt_root: value.receipt_root,
        }
    }
}

#[derive(Clone)]
pub struct StateEvents {
    pub events: Vec<Vec<StampedEvent>>,
}

// Various structures for implementing the tipset state cache

struct TipsetStateCacheInner<V> {
    values: LruCache<TipsetKey, V>,
    pending: Vec<(TipsetKey, Arc<TokioMutex<()>>)>,
}

impl<V: Clone> Default for TipsetStateCacheInner<V> {
    fn default() -> Self {
        Self {
            values: LruCache::new(DEFAULT_TIPSET_CACHE_SIZE),
            pending: Vec::with_capacity(8),
        }
    }
}

impl<V: Clone> TipsetStateCacheInner<V> {
    pub fn with_size(cache_size: NonZeroUsize) -> Self {
        Self {
            values: LruCache::new(cache_size),
            pending: Vec::with_capacity(8),
        }
    }
}

struct TipsetStateCache<V> {
    cache: Arc<SyncMutex<TipsetStateCacheInner<V>>>,
}

enum Status<V> {
    Done(V),
    Empty(Arc<TokioMutex<()>>),
}

impl<V: Clone> TipsetStateCache<V> {
    pub fn new() -> Self {
        Self {
            cache: Arc::new(SyncMutex::new(TipsetStateCacheInner::default())),
        }
    }

    pub fn with_size(cache_size: NonZeroUsize) -> Self {
        Self {
            cache: Arc::new(SyncMutex::new(TipsetStateCacheInner::with_size(cache_size))),
        }
    }

    fn with_inner<F, T>(&self, func: F) -> T
    where
        F: FnOnce(&mut TipsetStateCacheInner<V>) -> T,
    {
        let mut lock = self.cache.lock();
        func(&mut lock)
    }

    pub async fn get_or_else<F, Fut>(&self, key: &TipsetKey, compute: F) -> anyhow::Result<V>
    where
        F: Fn() -> Fut,
        Fut: core::future::Future<Output = anyhow::Result<V>>,
    {
        let status = self.with_inner(|inner| match inner.values.get(key) {
            Some(v) => Status::Done(v.clone()),
            None => {
                let option = inner
                    .pending
                    .iter()
                    .find(|(k, _)| k == key)
                    .map(|(_, mutex)| mutex);
                match option {
                    Some(mutex) => Status::Empty(mutex.clone()),
                    None => {
                        let mutex = Arc::new(TokioMutex::new(()));
                        inner.pending.push((key.clone(), mutex.clone()));
                        Status::Empty(mutex)
                    }
                }
            }
        });
        match status {
            Status::Done(x) => {
                crate::metrics::LRU_CACHE_HIT
                    .get_or_create(&crate::metrics::values::STATE_MANAGER_TIPSET)
                    .inc();
                Ok(x)
            }
            Status::Empty(mtx) => {
                let _guard = mtx.lock().await;
                match self.get(key) {
                    Some(v) => {
                        // While locking someone else computed the pending task
                        crate::metrics::LRU_CACHE_HIT
                            .get_or_create(&crate::metrics::values::STATE_MANAGER_TIPSET)
                            .inc();

                        Ok(v)
                    }
                    None => {
                        // Entry does not have state computed yet, compute value and fill the cache
                        crate::metrics::LRU_CACHE_MISS
                            .get_or_create(&crate::metrics::values::STATE_MANAGER_TIPSET)
                            .inc();

                        let value = compute().await?;

                        // Write back to cache, release lock and return value
                        self.insert(key.clone(), value.clone());
                        Ok(value)
                    }
                }
            }
        }
    }

    fn get(&self, key: &TipsetKey) -> Option<V> {
        self.with_inner(|inner| inner.values.get(key).cloned())
    }

    fn insert(&self, key: TipsetKey, value: V) {
        self.with_inner(|inner| {
            inner.pending.retain(|(k, _)| k != &key);
            inner.values.put(key, value);
        });
    }
}

/// External format for returning market balance from state.
#[derive(Default, Serialize, Deserialize, Clone, PartialEq, Eq, PartialOrd, Ord, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub struct MarketBalance {
    #[schemars(with = "LotusJson<TokenAmount>")]
    #[serde(with = "crate::lotus_json")]
    pub escrow: TokenAmount,
    #[schemars(with = "LotusJson<TokenAmount>")]
    #[serde(with = "crate::lotus_json")]
    pub locked: TokenAmount,
}
lotus_json_with_self!(MarketBalance);

/// State manager handles all interactions with the internal Filecoin actors
/// state. This encapsulates the [`ChainStore`] functionality, which only
/// handles chain data, to allow for interactions with the underlying state of
/// the chain. The state manager not only allows interfacing with state, but
/// also is used when performing state transitions.
pub struct StateManager<DB> {
    cs: Arc<ChainStore<DB>>,

    /// This is a cache which indexes tipsets to their calculated state.
    cache: TipsetStateCache<StateOutputValue>,
    /// This is a cache dedicated to tipset events.
    events_cache: TipsetStateCache<StateEvents>,
    // Beacon can be cheaply crated from the `chain_config`. The only reason we
    // store it here is because it has a look-up cache.
    beacon: Arc<crate::beacon::BeaconSchedule>,
    chain_config: Arc<ChainConfig>,
    engine: crate::shim::machine::MultiEngine,
}

#[allow(clippy::type_complexity)]
pub const NO_CALLBACK: Option<fn(MessageCallbackCtx<'_>) -> anyhow::Result<()>> = None;

impl<DB> StateManager<DB>
where
    DB: Blockstore,
{
    pub fn new(
        cs: Arc<ChainStore<DB>>,
        chain_config: Arc<ChainConfig>,
    ) -> Result<Self, anyhow::Error> {
        let genesis = cs.genesis_block_header();
        let beacon = Arc::new(chain_config.get_beacon_schedule(genesis.timestamp));

        Ok(Self {
            cs,
            cache: TipsetStateCache::new(),
            events_cache: TipsetStateCache::with_size(DEFAULT_EVENT_CACHE_SIZE),
            beacon,
            chain_config,
            engine: crate::shim::machine::MultiEngine::default(),
        })
    }

    pub fn beacon_schedule(&self) -> &Arc<BeaconSchedule> {
        &self.beacon
    }

    /// Returns network version for the given epoch.
    pub fn get_network_version(&self, epoch: ChainEpoch) -> NetworkVersion {
        self.chain_config.network_version(epoch)
    }

    pub fn chain_config(&self) -> &Arc<ChainConfig> {
        &self.chain_config
    }

    /// Gets the state tree
    pub fn get_state_tree(&self, state_cid: &Cid) -> anyhow::Result<StateTree<DB>> {
        StateTree::new_from_root(self.blockstore_owned(), state_cid)
    }

    /// Gets actor from given [`Cid`], if it exists.
    pub fn get_actor(&self, addr: &Address, state_cid: Cid) -> anyhow::Result<Option<ActorState>> {
        let state = self.get_state_tree(&state_cid)?;
        state.get_actor(addr)
    }

    /// Gets actor state from implicit actor address
    pub fn get_actor_state<S: LoadActorStateFromBlockstore>(
        &self,
        ts: &Tipset,
    ) -> anyhow::Result<S> {
        let state_tree = self.get_state_tree(ts.parent_state())?;
        state_tree.get_actor_state()
    }

    /// Gets actor state from explicit actor address
    pub fn get_actor_state_from_address<S: LoadActorStateFromBlockstore>(
        &self,
        ts: &Tipset,
        actor_address: &Address,
    ) -> anyhow::Result<S> {
        let state_tree = self.get_state_tree(ts.parent_state())?;
        state_tree.get_actor_state_from_address(actor_address)
    }

    /// Gets required actor from given [`Cid`].
    pub fn get_required_actor(&self, addr: &Address, state_cid: Cid) -> anyhow::Result<ActorState> {
        let state = self.get_state_tree(&state_cid)?;
        state.get_actor(addr)?.with_context(|| {
            format!("Failed to load actor with addr={addr}, state_cid={state_cid}")
        })
    }

    /// Returns a reference to the state manager's [`Blockstore`].
    pub fn blockstore(&self) -> &DB {
        self.cs.blockstore()
    }

    pub fn blockstore_owned(&self) -> Arc<DB> {
        Arc::clone(&self.cs.db)
    }

    /// Returns reference to the state manager's [`ChainStore`].
    pub fn chain_store(&self) -> &Arc<ChainStore<DB>> {
        &self.cs
    }

    pub fn chain_rand(&self, tipset: Arc<Tipset>) -> ChainRand<DB> {
        ChainRand::new(
            self.chain_config.clone(),
            tipset,
            self.cs.chain_index.clone(),
            self.beacon.clone(),
        )
    }

    /// Returns the internal, protocol-level network name.
    pub fn get_network_name_from_genesis(&self) -> anyhow::Result<String> {
        self.get_network_name(self.chain_store().genesis_block_header().state_root)
    }

    /// Returns the internal, protocol-level network name.
    pub fn get_network_name(&self, state_cid: Cid) -> anyhow::Result<String> {
        let init_act = self
            .get_actor(&init::ADDRESS.into(), state_cid)?
            .ok_or_else(|| Error::state("Init actor address could not be resolved"))?;
        Ok(State::load(self.blockstore(), init_act.code, init_act.state)?.into_network_name())
    }

    /// Returns true if miner has been slashed or is considered invalid.
    pub fn is_miner_slashed(&self, addr: &Address, state_cid: &Cid) -> anyhow::Result<bool, Error> {
        let actor = self
            .get_actor(&Address::POWER_ACTOR, *state_cid)?
            .ok_or_else(|| Error::state("Power actor address could not be resolved"))?;

        let spas = power::State::load(self.blockstore(), actor.code, actor.state)?;

        Ok(spas.miner_power(self.blockstore(), &addr.into())?.is_none())
    }

    /// Returns raw work address of a miner given the state root.
    pub fn get_miner_work_addr(&self, state_cid: Cid, addr: &Address) -> Result<Address, Error> {
        let state =
            StateTree::new_from_root(self.blockstore_owned(), &state_cid).map_err(Error::other)?;
        let ms: miner::State = state.get_actor_state_from_address(addr)?;
        let info = ms.info(self.blockstore()).map_err(|e| e.to_string())?;
        let addr = resolve_to_key_addr(&state, self.blockstore(), &info.worker().into())?;
        Ok(addr)
    }

    /// Returns specified actor's claimed power and total network power as a
    /// tuple.
    pub fn get_power(
        &self,
        state_cid: &Cid,
        addr: Option<&Address>,
    ) -> anyhow::Result<Option<(power::Claim, power::Claim)>, Error> {
        let actor = self
            .get_actor(&Address::POWER_ACTOR, *state_cid)?
            .ok_or_else(|| Error::state("Power actor address could not be resolved"))?;

        let spas = power::State::load(self.blockstore(), actor.code, actor.state)?;

        let t_pow = spas.total_power();

        if let Some(maddr) = addr {
            let m_pow = spas
                .miner_power(self.blockstore(), &maddr.into())?
                .ok_or_else(|| Error::state(format!("Miner for address {maddr} not found")))?;

            let min_pow = spas.miner_nominal_power_meets_consensus_minimum(
                &self.chain_config.policy,
                self.blockstore(),
                &maddr.into(),
            )?;
            if min_pow {
                return Ok(Some((m_pow, t_pow)));
            }
        }

        Ok(None)
    }

    // Returns all sectors
    pub fn get_all_sectors(
        self: &Arc<Self>,
        addr: &Address,
        ts: &Tipset,
    ) -> anyhow::Result<Vec<SectorOnChainInfo>> {
        let actor = self
            .get_actor(addr, *ts.parent_state())?
            .ok_or_else(|| Error::state("Miner actor not found"))?;
        let state = miner::State::load(self.blockstore(), actor.code, actor.state)?;
        Ok(state
            .load_sectors_ext(self.blockstore(), None)?
            .into_iter()
            .map(|(_, s)| s)
            .collect())
    }
}

impl<DB> StateManager<DB>
where
    DB: Blockstore + Send + Sync + 'static,
{
    /// Returns the pair of (parent state root, message receipt root). This will
    /// either be cached or will be calculated and fill the cache. Tipset
    /// state for a given tipset is guaranteed not to be computed twice.
    pub async fn tipset_state(self: &Arc<Self>, tipset: &Arc<Tipset>) -> anyhow::Result<CidPair> {
        let StateOutput {
            state_root,
            receipt_root,
            ..
        } = self.tipset_state_output(tipset).await?;
        Ok((state_root, receipt_root))
    }

    pub async fn tipset_state_output(
        self: &Arc<Self>,
        tipset: &Arc<Tipset>,
    ) -> anyhow::Result<StateOutput> {
        let key = tipset.key();
        self.cache
            .get_or_else(key, || async move {
                info!(
                    "Evaluating tipset: EPOCH = {}, blocks = {}",
                    tipset.epoch(),
                    tipset.len(),
                );
                let ts_state = self
                    .compute_tipset_state(
                        Arc::clone(tipset),
                        NO_CALLBACK,
                        VMTrace::NotTraced,
                        VMEvent::NotPushed,
                    )
                    .await?
                    .into();
                trace!("Completed tipset state calculation {:?}", tipset.cids());
                Ok(ts_state)
            })
            .await
            .map(StateOutput::from)
    }

    #[instrument(skip(self))]
    pub async fn tipset_state_events(
        self: &Arc<Self>,
        tipset: &Arc<Tipset>,
    ) -> anyhow::Result<StateEvents> {
        let key = tipset.key();
        self.events_cache
            .get_or_else(key, || async move {
                let ts_state = self
                    .compute_tipset_state(
                        Arc::clone(tipset),
                        NO_CALLBACK,
                        VMTrace::NotTraced,
                        VMEvent::Pushed,
                    )
                    .await?;
                trace!("Completed tipset state calculation {:?}", tipset.cids());
                Ok(StateEvents {
                    events: ts_state.events,
                })
            })
            .await
    }

    #[instrument(skip(self, rand))]
    fn call_raw(
        self: &Arc<Self>,
        msg: &Message,
        rand: ChainRand<DB>,
        tipset: &Arc<Tipset>,
    ) -> Result<ApiInvocResult, Error> {
        let mut msg = msg.clone();

        let state_cid = tipset.parent_state();

        let tipset_messages = self
            .chain_store()
            .messages_for_tipset(tipset)
            .map_err(|err| Error::Other(err.to_string()))?;

        let prior_messsages = tipset_messages
            .iter()
            .filter(|ts_msg| ts_msg.message().from() == msg.from());

        // Handle state forks
        // TODO(elmattic): https://github.com/ChainSafe/forest/issues/3733

        let height = tipset.epoch();
        let genesis_info = GenesisInfo::from_chain_config(self.chain_config().clone());
        let mut vm = VM::new(
            ExecutionContext {
                heaviest_tipset: Arc::clone(tipset),
                state_tree_root: *state_cid,
                epoch: height,
                rand: Box::new(rand),
                base_fee: tipset.block_headers().first().parent_base_fee.clone(),
                circ_supply: genesis_info.get_vm_circulating_supply(
                    height,
                    &self.blockstore_owned(),
                    state_cid,
                )?,
                chain_config: self.chain_config().clone(),
                chain_index: Arc::clone(&self.chain_store().chain_index),
                timestamp: tipset.min_timestamp(),
            },
            &self.engine,
            VMTrace::Traced,
        )?;

        for m in prior_messsages {
            vm.apply_message(m)?;
        }

        // We flush to get the VM's view of the state tree after applying the above messages
        // This is needed to get the correct nonce from the actor state to match the VM
        let state_cid = vm.flush()?;

        let state = StateTree::new_from_root(self.blockstore_owned(), &state_cid)?;

        let from_actor = state
            .get_actor(&msg.from())?
            .ok_or_else(|| anyhow::anyhow!("actor not found"))?;
        msg.set_sequence(from_actor.sequence);

        // If the fee cap is set to zero, make gas free
        // TODO(elmattic): https://github.com/ChainSafe/forest/issues/3733

        // Implicit messages need to set a special gas limit
        let mut msg = msg.clone();
        msg.gas_limit = IMPLICIT_MESSAGE_GAS_LIMIT as u64;

        let (apply_ret, duration) = vm.apply_implicit_message(&msg)?;

        Ok(ApiInvocResult {
            msg: msg.clone(),
            msg_rct: Some(apply_ret.msg_receipt()),
            msg_cid: msg.cid(),
            error: apply_ret.failure_info().unwrap_or_default(),
            duration: duration.as_nanos().clamp(0, u64::MAX as u128) as u64,
            gas_cost: MessageGasCost::default(),
            execution_trace: structured::parse_events(apply_ret.exec_trace()).unwrap_or_default(),
        })
    }

    /// runs the given message and returns its result without any persisted
    /// changes.
    pub fn call(
        self: &Arc<Self>,
        message: &Message,
        tipset: Option<Arc<Tipset>>,
    ) -> Result<ApiInvocResult, Error> {
        let ts = tipset.unwrap_or_else(|| self.cs.heaviest_tipset());
        let chain_rand = self.chain_rand(Arc::clone(&ts));
        self.call_raw(message, chain_rand, &ts)
    }

    /// Computes message on the given [Tipset] state, after applying other
    /// messages and returns the values computed in the VM.
    pub async fn call_with_gas(
        self: &Arc<Self>,
        message: &mut ChainMessage,
        prior_messages: &[ChainMessage],
        tipset: Option<Arc<Tipset>>,
        trace_config: VMTrace,
    ) -> Result<(InvocResult, ApplyRet), Error> {
        let ts = tipset.unwrap_or_else(|| self.cs.heaviest_tipset());
        let (st, _) = self
            .tipset_state(&ts)
            .await
            .map_err(|_| Error::Other("Could not load tipset state".to_string()))?;
        let chain_rand = self.chain_rand(Arc::clone(&ts));

        // Since we're simulating a future message, pretend we're applying it in the
        // "next" tipset
        let epoch = ts.epoch() + 1;
        let genesis_info = GenesisInfo::from_chain_config(self.chain_config().clone());
        // FVM requires a stack size of 64MiB. The alternative is to use `ThreadedExecutor` from
        // FVM, but that introduces some constraints, and possible deadlocks.
        let (ret, _) = stacker::grow(64 << 20, || -> ApplyResult {
            let mut vm = VM::new(
                ExecutionContext {
                    heaviest_tipset: Arc::clone(&ts),
                    state_tree_root: st,
                    epoch,
                    rand: Box::new(chain_rand),
                    base_fee: ts.block_headers().first().parent_base_fee.clone(),
                    circ_supply: genesis_info.get_vm_circulating_supply(
                        epoch,
                        &self.blockstore_owned(),
                        &st,
                    )?,
                    chain_config: self.chain_config().clone(),
                    chain_index: Arc::clone(&self.chain_store().chain_index),
                    timestamp: ts.min_timestamp(),
                },
                &self.engine,
                trace_config,
            )?;

            for msg in prior_messages {
                vm.apply_message(msg)?;
            }
            let from_actor = vm
                .get_actor(&message.from())
                .map_err(|e| Error::Other(format!("Could not get actor from state: {e}")))?
                .ok_or_else(|| Error::Other("cant find actor in state tree".to_string()))?;
            message.set_sequence(from_actor.sequence);

            vm.apply_message(message)
        })?;

        Ok((InvocResult::new(message.message().clone(), &ret), ret))
    }

    /// Replays the given message and returns the result of executing the
    /// indicated message, assuming it was executed in the indicated tipset.
    pub async fn replay(
        self: &Arc<Self>,
        ts: Arc<Tipset>,
        mcid: Cid,
    ) -> Result<ApiInvocResult, Error> {
        let this = Arc::clone(self);
        tokio::task::spawn_blocking(move || this.replay_blocking(ts, mcid))
            .await
            .map_err(|e| Error::Other(format!("{e}")))?
    }

    /// Blocking version of `replay`
    pub fn replay_blocking(
        self: &Arc<Self>,
        ts: Arc<Tipset>,
        mcid: Cid,
    ) -> Result<ApiInvocResult, Error> {
        const REPLAY_HALT: &str = "replay_halt";

        let mut api_invoc_result = None;
        let callback = |ctx: MessageCallbackCtx<'_>| {
            match ctx.at {
                CalledAt::Applied | CalledAt::Reward
                    if api_invoc_result.is_none() && ctx.cid == mcid =>
                {
                    api_invoc_result = Some(ApiInvocResult {
                        msg_cid: ctx.message.cid(),
                        msg: ctx.message.message().clone(),
                        msg_rct: Some(ctx.apply_ret.msg_receipt()),
                        error: ctx.apply_ret.failure_info().unwrap_or_default(),
                        duration: ctx.duration.as_nanos().clamp(0, u64::MAX as u128) as u64,
                        gas_cost: MessageGasCost::new(ctx.message.message(), ctx.apply_ret)?,
                        execution_trace: structured::parse_events(ctx.apply_ret.exec_trace())
                            .unwrap_or_default(),
                    });
                    anyhow::bail!(REPLAY_HALT);
                }
                _ => Ok(()), // ignored
            }
        };
        let result = self.compute_tipset_state_blocking(
            ts,
            Some(callback),
            VMTrace::Traced,
            VMEvent::NotPushed,
        );
        if let Err(error_message) = result {
            if error_message.to_string() != REPLAY_HALT {
                return Err(Error::Other(format!(
                    "unexpected error during execution : {error_message:}"
                )));
            }
        }
        api_invoc_result.ok_or_else(|| Error::Other("failed to replay".into()))
    }

    /// Checks the eligibility of the miner. This is used in the validation that
    /// a block's miner has the requirements to mine a block.
    pub fn eligible_to_mine(
        &self,
        address: &Address,
        base_tipset: &Tipset,
        lookback_tipset: &Tipset,
    ) -> anyhow::Result<bool, Error> {
        let hmp = self.miner_has_min_power(&self.chain_config.policy, address, lookback_tipset)?;
        let version = self.get_network_version(base_tipset.epoch());

        if version <= NetworkVersion::V3 {
            return Ok(hmp);
        }

        if !hmp {
            return Ok(false);
        }

        let actor = self
            .get_actor(&Address::POWER_ACTOR, *base_tipset.parent_state())?
            .ok_or_else(|| Error::state("Power actor address could not be resolved"))?;

        let power_state = power::State::load(self.blockstore(), actor.code, actor.state)?;

        let actor = self
            .get_actor(address, *base_tipset.parent_state())?
            .ok_or_else(|| Error::state("Miner actor address could not be resolved"))?;

        let miner_state = miner::State::load(self.blockstore(), actor.code, actor.state)?;

        // Non-empty power claim.
        let claim = power_state
            .miner_power(self.blockstore(), &address.into())?
            .ok_or_else(|| Error::Other("Could not get claim".to_string()))?;
        if claim.quality_adj_power <= BigInt::zero() {
            return Ok(false);
        }

        // No fee debt.
        if !miner_state.fee_debt().is_zero() {
            return Ok(false);
        }

        // No active consensus faults.
        let info = miner_state.info(self.blockstore())?;
        if base_tipset.epoch() <= info.consensus_fault_elapsed {
            return Ok(false);
        }

        Ok(true)
    }

    /// Conceptually, a [`Tipset`] consists of _blocks_ which share an _epoch_.
    /// Each _block_ contains _messages_, which are executed by the _Filecoin Virtual Machine_.
    ///
    /// VM message execution essentially looks like this:
    /// ```text
    /// state[N-900..N] * message = state[N+1]
    /// ```
    ///
    /// The `state`s above are stored in the `IPLD Blockstore`, and can be referred to by
    /// a [`Cid`] - the _state root_.
    /// The previous 900 states (configurable, see
    /// <https://docs.filecoin.io/reference/general/glossary/#finality>) can be
    /// queried when executing a message, so a store needs at least that many.
    /// (a snapshot typically contains 2000, for example).
    ///
    /// Each message costs FIL to execute - this is _gas_.
    /// After execution, the message has a _receipt_, showing how much gas was spent.
    /// This is similarly a [`Cid`] into the block store.
    ///
    /// For details, see the documentation for [`apply_block_messages`].
    ///
    #[instrument(skip_all)]
    pub async fn compute_tipset_state(
        self: &Arc<Self>,
        tipset: Arc<Tipset>,
        callback: Option<impl FnMut(MessageCallbackCtx<'_>) -> anyhow::Result<()> + Send + 'static>,
        enable_tracing: VMTrace,
        enable_event_pushing: VMEvent,
    ) -> Result<StateOutput, Error> {
        let this = Arc::clone(self);
        tokio::task::spawn_blocking(move || {
            this.compute_tipset_state_blocking(
                tipset,
                callback,
                enable_tracing,
                enable_event_pushing,
            )
        })
        .await?
    }

    /// Blocking version of `compute_tipset_state`
    #[tracing::instrument(skip_all)]
    pub fn compute_tipset_state_blocking(
        &self,
        tipset: Arc<Tipset>,
        callback: Option<impl FnMut(MessageCallbackCtx<'_>) -> anyhow::Result<()>>,
        enable_tracing: VMTrace,
        enable_event_pushing: VMEvent,
    ) -> Result<StateOutput, Error> {
        Ok(apply_block_messages(
            self.chain_store().genesis_block_header().timestamp,
            Arc::clone(&self.chain_store().chain_index),
            Arc::clone(&self.chain_config),
            self.beacon_schedule().clone(),
            &self.engine,
            tipset,
            callback,
            enable_tracing,
            enable_event_pushing,
        )?)
    }

    /// Check if tipset had executed the message, by loading the receipt based
    /// on the index of the message in the block.
    fn tipset_executed_message(
        &self,
        tipset: &Tipset,
        message: &ChainMessage,
        allow_replaced: bool,
    ) -> Result<Option<Receipt>, Error> {
        if tipset.epoch() == 0 {
            return Ok(None);
        }
        let message_from_address = message.from();
        let message_sequence = message.sequence();
        // Load parent state.
        let pts = self
            .cs
            .chain_index
            .load_required_tipset(tipset.parents())
            .map_err(|err| Error::Other(format!("Failed to load tipset: {err}")))?;
        let messages = self
            .cs
            .messages_for_tipset(&pts)
            .map_err(|err| Error::Other(format!("Failed to load messages for tipset: {err}")))?;
        messages
            .iter()
            .enumerate()
            // iterate in reverse because we going backwards through the chain
            .rev()
            .filter(|(_, s)| {
                s.sequence() == message_sequence
                    && s.from() == message_from_address
                    && s.equal_call(message)
            })
            .map(|(index, m)| {
                // A replacing message is a message with a different CID,
                // any of Gas values, and different signature, but with all
                // other parameters matching (source/destination, nonce, params, etc.)
                if !allow_replaced && message.cid() != m.cid(){
                    Err(Error::Other(format!(
                        "found message with equal nonce and call params but different CID. wanted {}, found: {}, nonce: {}, from: {}",
                        message.cid(),
                        m.cid(),
                        message.sequence(),
                        message.from(),
                    )))
                } else {
                    let block_header = tipset.block_headers().first();
                    crate::chain::get_parent_receipt(
                        self.blockstore(),
                        block_header,
                        index,
                    )
                    .map_err(|err| Error::Other(format!("Failed to get parent receipt (message_receipts={}, index={index}, error={err})", block_header.message_receipts)))
                }
            })
            .next()
            .unwrap_or(Ok(None))
    }

    fn check_search(
        &self,
        mut current: Arc<Tipset>,
        message: &ChainMessage,
        look_back_limit: Option<i64>,
        allow_replaced: Option<bool>,
    ) -> Result<Option<(Arc<Tipset>, Receipt)>, Error> {
        let allow_replaced = allow_replaced.unwrap_or(true);
        let message_from_address = message.from();
        let message_sequence = message.sequence();
        let mut current_actor_state = self
            .get_required_actor(&message_from_address, *current.parent_state())
            .map_err(Error::state)?;
        let message_from_id = self.lookup_required_id(&message_from_address, current.as_ref())?;
        while current.epoch() > look_back_limit.unwrap_or_default() {
            let parent_tipset = self
                .cs
                .chain_index
                .load_required_tipset(current.parents())
                .map_err(|err| {
                    Error::Other(format!(
                        "failed to load tipset during msg wait searchback: {err:}"
                    ))
                })?;

            let parent_actor_state = self
                .get_actor(&message_from_id, *parent_tipset.parent_state())
                .map_err(|e| Error::State(e.to_string()))?;

            if parent_actor_state.is_none()
                || (current_actor_state.sequence > message_sequence
                    && parent_actor_state.as_ref().unwrap().sequence <= message_sequence)
            {
                let receipt = self
                    .tipset_executed_message(current.as_ref(), message, allow_replaced)?
                    .context("Failed to get receipt with tipset_executed_message")?;
                return Ok(Some((current, receipt)));
            }

            if let Some(parent_actor_state) = parent_actor_state {
                current = parent_tipset;
                current_actor_state = parent_actor_state;
            } else {
                break;
            }
        }

        Ok(None)
    }

    fn search_back_for_message(
        &self,
        current: Arc<Tipset>,
        message: &ChainMessage,
        look_back_limit: Option<i64>,
        allow_replaced: Option<bool>,
    ) -> Result<Option<(Arc<Tipset>, Receipt)>, Error> {
        self.check_search(current, message, look_back_limit, allow_replaced)
    }

    /// Returns a message receipt from a given tipset and message CID.
    pub fn get_receipt(&self, tipset: Arc<Tipset>, msg: Cid) -> Result<Receipt, Error> {
        let m = crate::chain::get_chain_message(self.blockstore(), &msg)
            .map_err(|e| Error::Other(e.to_string()))?;
        let message_receipt = self.tipset_executed_message(&tipset, &m, true)?;
        if let Some(receipt) = message_receipt {
            return Ok(receipt);
        }

        let maybe_tuple = self.search_back_for_message(tipset, &m, None, None)?;
        let message_receipt = maybe_tuple
            .ok_or_else(|| {
                Error::Other("Could not get receipt from search back message".to_string())
            })?
            .1;
        Ok(message_receipt)
    }

    /// `WaitForMessage` blocks until a message appears on chain. It looks
    /// backwards in the chain to see if this has already happened. It
    /// guarantees that the message has been on chain for at least
    /// confidence epochs without being reverted before returning.
    pub async fn wait_for_message(
        self: &Arc<Self>,
        msg_cid: Cid,
        confidence: i64,
        look_back_limit: Option<ChainEpoch>,
        allow_replaced: Option<bool>,
    ) -> Result<(Option<Arc<Tipset>>, Option<Receipt>), Error> {
        let mut subscriber = self.cs.publisher().subscribe();
        let (sender, mut receiver) = oneshot::channel::<()>();
        let message = crate::chain::get_chain_message(self.blockstore(), &msg_cid)
            .map_err(|err| Error::Other(format!("failed to load message {err:}")))?;
        let current_tipset = self.cs.heaviest_tipset();
        let maybe_message_receipt =
            self.tipset_executed_message(&current_tipset, &message, true)?;
        if let Some(r) = maybe_message_receipt {
            return Ok((Some(current_tipset.clone()), Some(r)));
        }

        let mut candidate_tipset: Option<Arc<Tipset>> = None;
        let mut candidate_receipt: Option<Receipt> = None;

        let sm_cloned = Arc::clone(self);

        let message_for_task = message.clone();
        let height_of_head = current_tipset.epoch();
        let task = tokio::task::spawn(async move {
            let back_tuple = sm_cloned.search_back_for_message(
                current_tipset,
                &message_for_task,
                look_back_limit,
                allow_replaced,
            )?;
            sender
                .send(())
                .map_err(|e| Error::Other(format!("Could not send to channel {e:?}")))?;
            Ok::<_, Error>(back_tuple)
        });

        let reverts: Arc<RwLock<HashMap<TipsetKey, bool>>> = Arc::new(RwLock::new(HashMap::new()));
        let block_revert = reverts.clone();
        let sm_cloned = Arc::clone(self);

        // Wait for message to be included in head change.
        let mut subscriber_poll = tokio::task::spawn(async move {
            loop {
                match subscriber.recv().await {
                    Ok(subscriber) => match subscriber {
                        HeadChange::Apply(tipset) => {
                            if candidate_tipset
                                .as_ref()
                                .map(|s| tipset.epoch() >= s.epoch() + confidence)
                                .unwrap_or_default()
                            {
                                return Ok((candidate_tipset, candidate_receipt));
                            }
                            let poll_receiver = receiver.try_recv();
                            if let Ok(Some(_)) = poll_receiver {
                                block_revert
                                    .write()
                                    .await
                                    .insert(tipset.key().to_owned(), true);
                            }

                            let maybe_receipt =
                                sm_cloned.tipset_executed_message(&tipset, &message, true)?;
                            if let Some(receipt) = maybe_receipt {
                                if confidence == 0 {
                                    return Ok((Some(tipset), Some(receipt)));
                                }
                                candidate_tipset = Some(tipset);
                                candidate_receipt = Some(receipt)
                            }
                        }
                    },
                    Err(RecvError::Lagged(i)) => {
                        warn!(
                            "wait for message head change subscriber lagged, skipped {} events",
                            i
                        );
                    }
                    Err(RecvError::Closed) => break,
                }
            }
            Ok((None, None))
        })
        .fuse();

        // Search backwards for message.
        let mut search_back_poll = tokio::task::spawn(async move {
            let back_tuple = task.await.map_err(|e| {
                Error::Other(format!("Could not search backwards for message {e}"))
            })??;
            if let Some((back_tipset, back_receipt)) = back_tuple {
                let should_revert = *reverts
                    .read()
                    .await
                    .get(back_tipset.key())
                    .unwrap_or(&false);
                let larger_height_of_head = height_of_head >= back_tipset.epoch() + confidence;
                if !should_revert && larger_height_of_head {
                    return Ok::<_, Error>((Some(back_tipset), Some(back_receipt)));
                }
                return Ok((None, None));
            }
            Ok((None, None))
        })
        .fuse();

        // Await on first future to finish.
        loop {
            select! {
                res = subscriber_poll => {
                    return res?
                }
                res = search_back_poll => {
                    if let Ok((Some(ts), Some(rct))) = res? {
                        return Ok((Some(ts), Some(rct)));
                    }
                }
            }
        }
    }

    pub async fn search_for_message(
        self: &Arc<Self>,
        from: Option<Arc<Tipset>>,
        msg_cid: Cid,
        look_back_limit: Option<i64>,
        allow_replaced: Option<bool>,
    ) -> Result<Option<(Arc<Tipset>, Receipt)>, Error> {
        let from = from.unwrap_or_else(|| self.chain_store().heaviest_tipset());
        let message = crate::chain::get_chain_message(self.blockstore(), &msg_cid)
            .map_err(|err| Error::Other(format!("failed to load message {err}")))?;
        let current_tipset = self.cs.heaviest_tipset();
        let maybe_message_receipt =
            self.tipset_executed_message(&from, &message, allow_replaced.unwrap_or(true))?;
        if let Some(r) = maybe_message_receipt {
            Ok(Some((from, r)))
        } else {
            self.search_back_for_message(current_tipset, &message, look_back_limit, allow_replaced)
        }
    }

    /// Returns a BLS public key from provided address
    pub fn get_bls_public_key(
        db: &Arc<DB>,
        addr: &Address,
        state_cid: Cid,
    ) -> Result<BlsPublicKey, Error> {
        let state = StateTree::new_from_root(Arc::clone(db), &state_cid)
            .map_err(|e| Error::Other(e.to_string()))?;
        let kaddr = resolve_to_key_addr(&state, db, addr)
            .map_err(|e| format!("Failed to resolve key address, error: {e}"))?;

        match kaddr.into_payload() {
            Payload::BLS(key) => BlsPublicKey::from_bytes(&key)
                .map_err(|e| Error::Other(format!("Failed to construct bls public key: {e}"))),
            _ => Err(Error::state(
                "Address must be BLS address to load bls public key",
            )),
        }
    }

    /// Looks up ID [Address] from the state at the given [Tipset].
    pub fn lookup_id(&self, addr: &Address, ts: &Tipset) -> Result<Option<Address>, Error> {
        let state_tree = StateTree::new_from_root(self.blockstore_owned(), ts.parent_state())
            .map_err(|e| format!("{e:?}"))?;
        Ok(state_tree
            .lookup_id(addr)
            .map_err(|e| Error::Other(e.to_string()))?
            .map(Address::new_id))
    }

    /// Looks up required ID [Address] from the state at the given [Tipset].
    pub fn lookup_required_id(&self, addr: &Address, ts: &Tipset) -> Result<Address, Error> {
        self.lookup_id(addr, ts)?
            .ok_or_else(|| Error::Other(format!("Failed to lookup the id address {addr}")))
    }

    /// Retrieves market state
    pub fn market_state(&self, ts: &Tipset) -> Result<market::State, Error> {
        let actor = self.get_required_actor(&Address::MARKET_ACTOR, *ts.parent_state())?;
        let market_state = market::State::load(self.blockstore(), actor.code, actor.state)?;
        Ok(market_state)
    }

    /// Retrieves market balance in escrow and locked tables.
    pub fn market_balance(&self, addr: &Address, ts: &Tipset) -> Result<MarketBalance, Error> {
        let market_state = self.market_state(ts)?;
        let new_addr = self.lookup_required_id(addr, ts)?;
        let out = MarketBalance {
            escrow: {
                market_state
                    .escrow_table(self.blockstore())?
                    .get(&new_addr.into())?
                    .into()
            },
            locked: {
                market_state
                    .locked_table(self.blockstore())?
                    .get(&new_addr.into())?
                    .into()
            },
        };

        Ok(out)
    }

    /// Retrieves miner info.
    pub fn miner_info(&self, addr: &Address, ts: &Tipset) -> Result<MinerInfo, Error> {
        let actor = self
            .get_actor(addr, *ts.parent_state())?
            .ok_or_else(|| Error::state("Miner actor not found"))?;
        let state = miner::State::load(self.blockstore(), actor.code, actor.state)?;

        Ok(state.info(self.blockstore())?)
    }

    /// Retrieves miner faults.
    pub fn miner_faults(&self, addr: &Address, ts: &Tipset) -> Result<BitField, Error> {
        self.all_partition_sectors(addr, ts, |partition| partition.faulty_sectors().clone())
    }

    /// Retrieves miner recoveries.
    pub fn miner_recoveries(&self, addr: &Address, ts: &Tipset) -> Result<BitField, Error> {
        self.all_partition_sectors(addr, ts, |partition| partition.recovering_sectors().clone())
    }

    fn all_partition_sectors(
        &self,
        addr: &Address,
        ts: &Tipset,
        get_sector: impl Fn(Partition<'_>) -> BitField,
    ) -> Result<BitField, Error> {
        let actor = self
            .get_actor(addr, *ts.parent_state())?
            .ok_or_else(|| Error::state("Miner actor not found"))?;

        let state = miner::State::load(self.blockstore(), actor.code, actor.state)?;

        let mut partitions = Vec::new();

        state.for_each_deadline(
            &self.chain_config.policy,
            self.blockstore(),
            |_, deadline| {
                deadline.for_each(self.blockstore(), |_, partition| {
                    partitions.push(get_sector(partition));
                    Ok(())
                })
            },
        )?;

        Ok(BitField::union(partitions.iter()))
    }

    /// Retrieves miner power.
    pub fn miner_power(&self, addr: &Address, ts: &Tipset) -> Result<MinerPower, Error> {
        if let Some((miner_power, total_power)) = self.get_power(ts.parent_state(), Some(addr))? {
            return Ok(MinerPower {
                miner_power,
                total_power,
                has_min_power: true,
            });
        }

        Ok(MinerPower {
            has_min_power: false,
            miner_power: Default::default(),
            total_power: Default::default(),
        })
    }

    /// Similar to `resolve_to_key_addr` in the `forest_vm` [`crate::state_manager`] but does not
    /// allow `Actor` type of addresses. Uses `ts` to generate the VM state.
    pub async fn resolve_to_key_addr(
        self: &Arc<Self>,
        addr: &Address,
        ts: &Arc<Tipset>,
    ) -> Result<Address, anyhow::Error> {
        match addr.protocol() {
            Protocol::BLS | Protocol::Secp256k1 | Protocol::Delegated => return Ok(*addr),
            Protocol::Actor => {
                return Err(
                    Error::Other("cannot resolve actor address to key address".to_string()).into(),
                )
            }
            _ => {}
        };

        // First try to resolve the actor in the parent state, so we don't have to
        // compute anything.
        let state = StateTree::new_from_root(self.blockstore_owned(), ts.parent_state())?;
        if let Ok(addr) = resolve_to_key_addr(&state, self.blockstore(), addr) {
            return Ok(addr);
        }

        // If that fails, compute the tip-set and try again.
        let (st, _) = self.tipset_state(ts).await?;
        let state = StateTree::new_from_root(self.blockstore_owned(), &st)?;

        resolve_to_key_addr(&state, self.blockstore(), addr)
    }

    pub async fn miner_get_base_info(
        self: &Arc<Self>,
        beacon_schedule: &BeaconSchedule,
        tipset: Arc<Tipset>,
        addr: Address,
        epoch: ChainEpoch,
    ) -> anyhow::Result<Option<MiningBaseInfo>> {
        let prev_beacon = self
            .chain_store()
            .chain_index
            .latest_beacon_entry(tipset.clone())?;

        let entries: Vec<BeaconEntry> = beacon_schedule
            .beacon_entries_for_block(
                self.chain_config.network_version(epoch),
                epoch,
                tipset.epoch(),
                &prev_beacon,
            )
            .await?;

        let base = entries.last().unwrap_or(&prev_beacon);

        let (lb_tipset, lb_state_root) = ChainStore::get_lookback_tipset_for_round(
            self.cs.chain_index.clone(),
            self.chain_config.clone(),
            tipset.clone(),
            epoch,
        )?;

        // If the miner actor doesn't exist in the current tipset, it is a
        // user-error and we must return an error message. If the miner exists
        // in the current tipset but not in the lookback tipset, we may not
        // error and should instead return None.
        let actor = self.get_required_actor(&addr, *tipset.parent_state())?;
        if self.get_actor(&addr, lb_state_root)?.is_none() {
            return Ok(None);
        }

        let miner_state = miner::State::load(self.blockstore(), actor.code, actor.state)?;

        let addr_buf = to_vec(&addr)?;
        let rand = draw_randomness(
            base.signature(),
            DomainSeparationTag::WinningPoStChallengeSeed as i64,
            epoch,
            &addr_buf,
        )?;

        let network_version = self.chain_config.network_version(tipset.epoch());
        let sectors = self.get_sectors_for_winning_post(
            &lb_state_root,
            network_version,
            &addr,
            Randomness::new(rand.to_vec()),
        )?;

        if sectors.is_empty() {
            return Ok(None);
        }

        let (miner_power, total_power) = self
            .get_power(&lb_state_root, Some(&addr))?
            .context("failed to get power")?;

        let info = miner_state.info(self.blockstore())?;

        let worker_key = self
            .resolve_to_deterministic_address(info.worker.into(), tipset.clone())
            .await?;
        let eligible = self.eligible_to_mine(&addr, &tipset, &lb_tipset)?;

        Ok(Some(MiningBaseInfo {
            miner_power: miner_power.quality_adj_power,
            network_power: total_power.quality_adj_power,
            sectors,
            worker_key,
            sector_size: info.sector_size,
            prev_beacon_entry: prev_beacon,
            beacon_entries: entries,
            eligible_for_mining: eligible,
        }))
    }

    /// Checks power actor state for if miner meets consensus minimum
    /// requirements.
    pub fn miner_has_min_power(
        &self,
        policy: &Policy,
        addr: &Address,
        ts: &Tipset,
    ) -> anyhow::Result<bool> {
        let actor = self
            .get_actor(&Address::POWER_ACTOR, *ts.parent_state())?
            .ok_or_else(|| Error::state("Power actor address could not be resolved"))?;
        let ps = power::State::load(self.blockstore(), actor.code, actor.state)?;

        ps.miner_nominal_power_meets_consensus_minimum(policy, self.blockstore(), &addr.into())
    }

    /// Validates all tipsets at epoch `start..=end` behind the heaviest tipset.
    ///
    /// This spawns [`rayon::current_num_threads`] threads to do the compute-heavy work
    /// of tipset validation.
    ///
    /// # What is validation?
    /// Every state transition returns a new _state root_, which is typically retained in, e.g., snapshots.
    /// For "full" snapshots, all state roots are retained.
    /// For standard snapshots, the last 2000 or so state roots are retained.
    ///
    /// _receipts_ meanwhile, are typically ephemeral, but each tipset knows the _receipt root_
    /// (hash) of the previous tipset.
    ///
    /// This function takes advantage of that fact to validate tipsets:
    /// - `tipset[N]` claims that `receipt_root[N-1]` should be `0xDEADBEEF`
    /// - find `tipset[N-1]`, and perform its state transition to get the actual `receipt_root`
    /// - assert that they match
    ///
    /// See [`Self::compute_tipset_state_blocking`] for an explanation of state transitions.
    ///
    /// # Known issues
    /// This function is blocking, but we do observe threads waiting and synchronizing.
    /// This is suspected to be due something in the VM or its `WASM` runtime.
    #[tracing::instrument(skip(self))]
    pub fn validate_range(self: &Arc<Self>, epochs: RangeInclusive<i64>) -> anyhow::Result<()> {
        let heaviest = self.cs.heaviest_tipset();
        let heaviest_epoch = heaviest.epoch();
        let end = self
            .cs
            .chain_index
            .tipset_by_height(*epochs.end(), heaviest, ResolveNullTipset::TakeOlder)
            .with_context(|| {
                format!(
            "couldn't get a tipset at height {} behind heaviest tipset at height {heaviest_epoch}",
            *epochs.end(),
        )
            })?;

        // lookup tipset parents as we go along, iterating DOWN from `end`
        let tipsets = self
            .cs
            .chain_index
            .chain(end)
            .take_while(|tipset| tipset.epoch() >= *epochs.start());

        self.validate_tipsets(tipsets)
    }

    pub fn validate_tipsets<T>(self: &Arc<Self>, tipsets: T) -> anyhow::Result<()>
    where
        T: Iterator<Item = Arc<Tipset>> + Send,
    {
        let genesis_timestamp = self.chain_store().genesis_block_header().timestamp;
        validate_tipsets(
            genesis_timestamp,
            self.chain_store().chain_index.clone(),
            self.chain_config().clone(),
            self.beacon_schedule().clone(),
            &self.engine,
            tipsets,
        )
    }

    pub fn get_verified_registry_actor_state(
        &self,
        ts: &Tipset,
    ) -> anyhow::Result<verifreg::State> {
        let act = self
            .get_actor(&Address::VERIFIED_REGISTRY_ACTOR, *ts.parent_state())
            .map_err(Error::state)?
            .ok_or_else(|| Error::state("actor not found"))?;
        verifreg::State::load(self.blockstore(), act.code, act.state)
    }
    pub fn get_claim(
        &self,
        addr: &Address,
        ts: &Tipset,
        claim_id: ClaimID,
    ) -> anyhow::Result<Option<Claim>> {
        let id_address = self.lookup_required_id(addr, ts)?;
        let state = self.get_verified_registry_actor_state(ts)?;
        state.get_claim(self.blockstore(), id_address.into(), claim_id)
    }

    pub fn get_all_claims(&self, ts: &Tipset) -> anyhow::Result<HashMap<ClaimID, Claim>> {
        let state = self.get_verified_registry_actor_state(ts)?;
        state.get_all_claims(self.blockstore())
    }

    pub fn get_allocation(
        &self,
        addr: &Address,
        ts: &Tipset,
        allocation_id: AllocationID,
    ) -> anyhow::Result<Option<Allocation>> {
        let id_address = self.lookup_required_id(addr, ts)?;
        let state = self.get_verified_registry_actor_state(ts)?;
        state.get_allocation(self.blockstore(), id_address.id()?, allocation_id)
    }

    pub fn get_all_allocations(
        &self,
        ts: &Tipset,
    ) -> anyhow::Result<HashMap<AllocationID, Allocation>> {
        let state = self.get_verified_registry_actor_state(ts)?;
        state.get_all_allocations(self.blockstore())
    }

    pub fn verified_client_status(
        &self,
        addr: &Address,
        ts: &Tipset,
    ) -> anyhow::Result<Option<DataCap>> {
        let id = self.lookup_required_id(addr, ts)?;
        let network_version = self.get_network_version(ts.epoch());

        // This is a copy of Lotus code, we need to treat all the actors below version 9
        // differently. Which maps to network below version 17.
        // Original: https://github.com/filecoin-project/lotus/blob/5e76b05b17771da6939c7b0bf65127c3dc70ee23/node/impl/full/state.go#L1627-L1664.
        if (u32::from(network_version.0)) < 17 {
            let state = self.get_verified_registry_actor_state(ts)?;
            return state.verified_client_data_cap(self.blockstore(), id.into());
        }

        let act = self
            .get_actor(&Address::DATACAP_TOKEN_ACTOR, *ts.parent_state())
            .map_err(Error::state)?
            .ok_or_else(|| Error::state("Miner actor not found"))?;

        let state = datacap::State::load(self.blockstore(), act.code, act.state)?;

        state.verified_client_data_cap(self.blockstore(), id.into())
    }

    pub async fn resolve_to_deterministic_address(
        self: &Arc<Self>,
        address: Address,
        ts: Arc<Tipset>,
    ) -> anyhow::Result<Address> {
        use crate::shim::address::Protocol::*;
        match address.protocol() {
            BLS | Secp256k1 | Delegated => Ok(address),
            Actor => anyhow::bail!("cannot resolve actor address to key address"),
            _ => {
                // First try to resolve the actor in the parent state, so we don't have to compute anything.
                if let Ok(state) =
                    StateTree::new_from_root(self.chain_store().db.clone(), ts.parent_state())
                {
                    if let Ok(address) = state
                        .resolve_to_deterministic_addr(self.chain_store().blockstore(), address)
                    {
                        return Ok(address);
                    }
                }

                // If that fails, compute the tip-set and try again.
                let (state_root, _) = self.tipset_state(&ts).await?;
                let state = StateTree::new_from_root(self.chain_store().db.clone(), &state_root)?;
                state.resolve_to_deterministic_addr(self.chain_store().blockstore(), address)
            }
        }
    }

    pub fn execution_trace(&self, tipset: &Tipset) -> anyhow::Result<(Cid, Vec<ApiInvocResult>)> {
        let mut invoc_trace = vec![];

        let genesis_timestamp = self.chain_store().genesis_block_header().timestamp;

        let callback = |ctx: MessageCallbackCtx<'_>| {
            match ctx.at {
                CalledAt::Applied | CalledAt::Reward => {
                    invoc_trace.push(ApiInvocResult {
                        msg_cid: ctx.message.cid(),
                        msg: ctx.message.message().clone(),
                        msg_rct: Some(ctx.apply_ret.msg_receipt()),
                        error: ctx.apply_ret.failure_info().unwrap_or_default(),
                        duration: ctx.duration.as_nanos().clamp(0, u64::MAX as u128) as u64,
                        gas_cost: MessageGasCost::new(ctx.message.message(), ctx.apply_ret)?,
                        execution_trace: structured::parse_events(ctx.apply_ret.exec_trace())
                            .unwrap_or_default(),
                    });
                    Ok(())
                }
                _ => Ok(()), // ignored
            }
        };

        let StateOutput { state_root, .. } = apply_block_messages(
            genesis_timestamp,
            self.chain_store().chain_index.clone(),
            self.chain_config().clone(),
            self.beacon_schedule().clone(),
            &self.engine,
            Arc::new(tipset.clone()),
            Some(callback),
            VMTrace::Traced,
            VMEvent::NotPushed,
        )?;

        Ok((state_root, invoc_trace))
    }
}

pub fn validate_tipsets<DB, T>(
    genesis_timestamp: u64,
    chain_index: Arc<ChainIndex<Arc<DB>>>,
    chain_config: Arc<ChainConfig>,
    beacon: Arc<BeaconSchedule>,
    engine: &crate::shim::machine::MultiEngine,
    tipsets: T,
) -> anyhow::Result<()>
where
    DB: Blockstore + Send + Sync + 'static,
    T: Iterator<Item = Arc<Tipset>> + Send,
{
    use rayon::iter::ParallelIterator as _;
    tipsets
        .tuple_windows()
        .par_bridge()
        .try_for_each(|(child, parent)| {
            info!(height = parent.epoch(), "compute parent state");
            let StateOutput {
                state_root: actual_state,
                receipt_root: actual_receipt,
                ..
            } = apply_block_messages(
                genesis_timestamp,
                chain_index.clone(),
                chain_config.clone(),
                beacon.clone(),
                engine,
                parent,
                NO_CALLBACK,
                VMTrace::NotTraced,
                VMEvent::NotPushed,
            )
            .context("couldn't compute tipset state")?;
            let expected_receipt = child.min_ticket_block().message_receipts;
            let expected_state = child.parent_state();
            match (expected_state, expected_receipt) == (&actual_state, actual_receipt) {
                true => Ok(()),
                false => {
                    error!(
                        height = child.epoch(),
                        ?expected_state,
                        ?expected_receipt,
                        ?actual_state,
                        ?actual_receipt,
                        "state mismatch"
                    );
                    bail!("state mismatch");
                }
            }
        })
}

/// Messages are transactions that produce new states. The state (usually
/// referred to as the 'state-tree') is a mapping from actor addresses to actor
/// states. Each block contains the hash of the state-tree that should be used
/// as the starting state when executing the block messages.
///
/// # Execution environment
///
/// Transaction execution has the following inputs:
/// - a current state-tree (stored as IPLD in a key-value database). This
///   reference is in [`Tipset::parent_state`].
/// - up to 900 past state-trees. See
///   <https://docs.filecoin.io/reference/general/glossary/#finality>.
/// - up to 900 past tipset IDs.
/// - a deterministic source of randomness.
/// - the circulating supply of FIL (see
///   <https://filecoin.io/blog/filecoin-circulating-supply/>). The circulating
///   supply is determined by the epoch and the states of a few key actors.
/// - the base fee (see <https://spec.filecoin.io/systems/filecoin_vm/gas_fee/>).
///   This value is defined by `tipset.parent_base_fee`.
/// - the genesis timestamp (UNIX epoch time when the first block was
///   mined/created).
/// - a chain configuration (maps epoch to network version, has chain specific
///   settings).
///
/// The result of running a set of block messages is an index to the final
/// state-tree and an index to an array of message receipts (listing gas used,
/// return codes, etc).
///
/// # Cron and null tipsets
///
/// Once per epoch, after all messages have run, a special 'cron' transaction
/// must be executed. The tasks of the 'cron' transaction include running batch
/// jobs and keeping the state up-to-date with the current epoch.
///
/// It can happen that no blocks are mined in an epoch. The tipset for such an
/// epoch is called a null tipset. A null tipset has no identity and cannot be
/// directly executed. This is a problem for 'cron' which must run for every
/// epoch, even if there are no messages. The fix is to run 'cron' if there are
/// any null tipsets between the current epoch and the parent epoch.
///
/// Imagine the blockchain looks like this with a null tipset at epoch 9:
///
/// ```text
/// ┌────────┐ ┌────┐ ┌───────┐  ┌───────┐
/// │Epoch 10│ │Null│ │Epoch 8├──►Epoch 7├─►
/// └───┬────┘ └────┘ └───▲───┘  └───────┘
///     └─────────────────┘
/// ```
///
/// The parent of tipset-epoch-10 is tipset-epoch-8. Before executing the
/// messages in epoch 10, we have to run cron for epoch 9. However, running
/// 'cron' requires the timestamp of the youngest block in the tipset (which
/// doesn't exist because there are no blocks in the tipset). Lotus dictates that
/// the timestamp of a null tipset is `30s * epoch` after the genesis timestamp.
/// So, in the above example, if the genesis block was mined at time `X`, the
/// null tipset for epoch 9 will have timestamp `X + 30 * 9`.
///
/// # Migrations
///
/// Migrations happen between network upgrades and modify the state tree. If a
/// migration is scheduled for epoch 10, it will be run _after_ the messages for
/// epoch 10. The tipset for epoch 11 will link the state-tree produced by the
/// migration.
///
/// Example timeline with a migration at epoch 10:
///   1. Tipset-epoch-10 executes, producing state-tree A.
///   2. Migration consumes state-tree A and produces state-tree B.
///   3. Tipset-epoch-11 executes, consuming state-tree B (rather than A).
///
/// Note: The migration actually happens when tipset-epoch-11 executes. This is
///       because tipset-epoch-10 may be null and therefore not executed at all.
///
/// # Caching
///
/// Scanning the blockchain to find past tipsets and state-trees may be slow.
/// The `ChainStore` caches recent tipsets to make these scans faster.
#[allow(clippy::too_many_arguments)]
pub fn apply_block_messages<DB>(
    genesis_timestamp: u64,
    chain_index: Arc<ChainIndex<Arc<DB>>>,
    chain_config: Arc<ChainConfig>,
    beacon: Arc<BeaconSchedule>,
    engine: &crate::shim::machine::MultiEngine,
    tipset: Arc<Tipset>,
    mut callback: Option<impl FnMut(MessageCallbackCtx<'_>) -> anyhow::Result<()>>,
    enable_tracing: VMTrace,
    enable_event_pushing: VMEvent,
) -> Result<StateOutput, anyhow::Error>
where
    DB: Blockstore + Send + Sync + 'static,
{
    // This function will:
    // 1. handle the genesis block as a special case
    // 2. run 'cron' for any null-tipsets between the current tipset and our parent tipset
    // 3. run migrations
    // 4. execute block messages
    // 5. write the state-tree to the DB and return the CID

    // step 1: special case for genesis block
    if tipset.epoch() == 0 {
        // NB: This is here because the process that executes blocks requires that the
        // block miner reference a valid miner in the state tree. Unless we create some
        // magical genesis miner, this won't work properly, so we short circuit here
        // This avoids the question of 'who gets paid the genesis block reward'
        let message_receipts = tipset.min_ticket_block().message_receipts;
        return Ok(StateOutput {
            state_root: *tipset.parent_state(),
            receipt_root: message_receipts,
            events: vec![],
        });
    }

    let rand = ChainRand::new(
        Arc::clone(&chain_config),
        Arc::clone(&tipset),
        Arc::clone(&chain_index),
        beacon,
    );

    let genesis_info = GenesisInfo::from_chain_config(chain_config.clone());
    let create_vm = |state_root: Cid, epoch, timestamp| {
        let circulating_supply =
            genesis_info.get_vm_circulating_supply(epoch, &chain_index.db, &state_root)?;
        VM::new(
            ExecutionContext {
                heaviest_tipset: Arc::clone(&tipset),
                state_tree_root: state_root,
                epoch,
                rand: Box::new(rand.clone()),
                base_fee: tipset.min_ticket_block().parent_base_fee.clone(),
                circ_supply: circulating_supply,
                chain_config: Arc::clone(&chain_config),
                chain_index: Arc::clone(&chain_index),
                timestamp,
            },
            engine,
            enable_tracing,
        )
    };

    let mut parent_state = *tipset.parent_state();

    let parent_epoch = Tipset::load_required(&chain_index.db, tipset.parents())?.epoch();
    let epoch = tipset.epoch();

    for epoch_i in parent_epoch..epoch {
        if epoch_i > parent_epoch {
            // step 2: running cron for any null-tipsets
            let timestamp = genesis_timestamp + ((EPOCH_DURATION_SECONDS * epoch_i) as u64);

            // FVM requires a stack size of 64MiB. The alternative is to use `ThreadedExecutor` from
            // FVM, but that introduces some constraints, and possible deadlocks.
            parent_state = stacker::grow(64 << 20, || -> anyhow::Result<Cid> {
                let mut vm = create_vm(parent_state, epoch_i, timestamp)?;
                // run cron for null rounds if any
                if let Err(e) = vm.run_cron(epoch_i, callback.as_mut()) {
                    error!("Beginning of epoch cron failed to run: {}", e);
                }
                vm.flush()
            })?;
        }

        // step 3: run migrations
        if let Some(new_state) =
            run_state_migrations(epoch_i, &chain_config, &chain_index.db, &parent_state)?
        {
            parent_state = new_state;
        }
    }

    let block_messages = BlockMessages::for_tipset(&chain_index.db, &tipset)
        .map_err(|e| Error::Other(e.to_string()))?;

    // FVM requires a stack size of 64MiB. The alternative is to use `ThreadedExecutor` from
    // FVM, but that introduces some constraints, and possible deadlocks.
    stacker::grow(64 << 20, || -> anyhow::Result<StateOutput> {
        let mut vm = create_vm(parent_state, epoch, tipset.min_timestamp())?;

        // step 4: apply tipset messages
        let (receipts, events) =
            vm.apply_block_messages(&block_messages, epoch, callback, enable_event_pushing)?;

        // step 5: construct receipt root from receipts and flush the state-tree
        let receipt_root = Amt::new_from_iter(&chain_index.db, receipts)?;
        let state_root = vm.flush()?;

        Ok(StateOutput {
            state_root,
            receipt_root,
            events,
        })
    })
}
