// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod chain_rand;
mod errors;
mod metrics;
mod utils;
use crate::state_migration::run_state_migrations;
use anyhow::{bail, Context as _};
use rayon::prelude::ParallelBridge;
pub use utils::is_valid_for_sending;
mod vm_circ_supply;
pub use self::errors::*;
use crate::beacon::{BeaconSchedule, DrandBeacon};
use crate::blocks::{Tipset, TipsetKeys};
use crate::chain::{
    index::{ChainIndex, ResolveNullTipset},
    ChainStore, HeadChange,
};
use crate::interpreter::BlockMessages;
use crate::interpreter::{resolve_to_key_addr, ExecutionContext, VM};
use crate::json::message_receipt;
use crate::message::{ChainMessage, Message as MessageTrait};
use crate::networks::ChainConfig;
use crate::shim::clock::ChainEpoch;
use crate::shim::{
    address::{Address, Payload, Protocol, BLS_PUB_LEN},
    econ::TokenAmount,
    executor::{ApplyRet, Receipt},
    message::Message,
    state_tree::{ActorState, StateTree},
    version::NetworkVersion,
};
use ahash::{HashMap, HashMapExt};
use chain_rand::ChainRand;
use cid::Cid;
use fil_actor_interface::*;
use fil_actors_shared::v10::runtime::Policy;
use futures::{channel::oneshot, select, FutureExt};
use fvm_ipld_amt::Amtv0 as Amt;
use fvm_ipld_blockstore::Blockstore;
use itertools::Itertools as _;
use lru::LruCache;
use nonzero_ext::nonzero;
use num::BigInt;
use num_traits::identities::Zero;
use parking_lot::Mutex as SyncMutex;
use serde::{Deserialize, Serialize};
use std::ops::RangeInclusive;
use std::{num::NonZeroUsize, sync::Arc};
use tokio::sync::{broadcast::error::RecvError, Mutex as TokioMutex, RwLock};
use tracing::{debug, error, info, instrument, trace, warn};
use vm_circ_supply::GenesisInfo;

const DEFAULT_TIPSET_CACHE_SIZE: NonZeroUsize = nonzero!(1024usize);

/// Intermediary for retrieving state objects and updating actor states.
type CidPair = (Cid, Cid);

// Various structures for implementing the tipset state cache

struct TipsetStateCacheInner {
    values: LruCache<TipsetKeys, CidPair>,
    pending: Vec<(TipsetKeys, Arc<TokioMutex<()>>)>,
}

impl Default for TipsetStateCacheInner {
    fn default() -> Self {
        Self {
            values: LruCache::new(DEFAULT_TIPSET_CACHE_SIZE),
            pending: Vec::with_capacity(8),
        }
    }
}

struct TipsetStateCache {
    cache: Arc<SyncMutex<TipsetStateCacheInner>>,
}

enum Status {
    Done(CidPair),
    Empty(Arc<TokioMutex<()>>),
}

impl TipsetStateCache {
    pub fn new() -> Self {
        Self {
            cache: Arc::new(SyncMutex::new(TipsetStateCacheInner::default())),
        }
    }

    fn with_inner<F, T>(&self, func: F) -> T
    where
        F: FnOnce(&mut TipsetStateCacheInner) -> T,
    {
        let mut lock = self.cache.lock();
        func(&mut lock)
    }

    pub async fn get_or_else<F, Fut>(&self, key: &TipsetKeys, compute: F) -> anyhow::Result<CidPair>
    where
        F: Fn() -> Fut,
        Fut: core::future::Future<Output = anyhow::Result<CidPair>>,
    {
        let status = self.with_inner(|inner| match inner.values.get(key) {
            Some(v) => Status::Done(*v),
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
                    .with_label_values(&[crate::metrics::values::STATE_MANAGER_TIPSET])
                    .inc();
                Ok(x)
            }
            Status::Empty(mtx) => {
                let _guard = mtx.lock().await;
                match self.get(key) {
                    Some(v) => {
                        // While locking someone else computed the pending task
                        crate::metrics::LRU_CACHE_HIT
                            .with_label_values(&[crate::metrics::values::STATE_MANAGER_TIPSET])
                            .inc();

                        Ok(v)
                    }
                    None => {
                        // Entry does not have state computed yet, compute value and fill the cache
                        crate::metrics::LRU_CACHE_MISS
                            .with_label_values(&[crate::metrics::values::STATE_MANAGER_TIPSET])
                            .inc();

                        let cid_pair = compute().await?;

                        // Write back to cache, release lock and return value
                        self.insert(key.clone(), cid_pair);
                        Ok(cid_pair)
                    }
                }
            }
        }
    }

    fn get(&self, key: &TipsetKeys) -> Option<CidPair> {
        self.with_inner(|inner| inner.values.get(key).copied())
    }

    fn insert(&self, key: TipsetKeys, value: CidPair) {
        self.with_inner(|inner| {
            inner.pending.retain(|(k, _)| k != &key);
            inner.values.put(key, value);
        });
    }
}

/// Type to represent invocation of state call results.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct InvocResult {
    #[serde(with = "crate::json::message::json")]
    pub msg: Message,
    #[serde(with = "message_receipt::json::opt")]
    pub msg_rct: Option<Receipt>,
    pub error: Option<String>,
}

/// An alias Result that represents an `InvocResult` and an Error.
type StateCallResult = Result<InvocResult, Error>;

/// External format for returning market balance from state.
#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct MarketBalance {
    escrow: TokenAmount,
    locked: TokenAmount,
}

/// State manager handles all interactions with the internal Filecoin actors
/// state. This encapsulates the [`ChainStore`] functionality, which only
/// handles chain data, to allow for interactions with the underlying state of
/// the chain. The state manager not only allows interfacing with state, but
/// also is used when performing state transitions.
pub struct StateManager<DB> {
    cs: Arc<ChainStore<DB>>,

    /// This is a cache which indexes tipsets to their calculated state.
    cache: TipsetStateCache,
    // Beacon can be cheaply crated from the `chain_config`. The only reason we
    // store it here is because it has a look-up cache.
    beacon: Arc<crate::beacon::BeaconSchedule<DrandBeacon>>,
    chain_config: Arc<ChainConfig>,
    engine: crate::shim::machine::MultiEngine,
}

#[allow(clippy::type_complexity)]
pub const NO_CALLBACK: Option<fn(&Cid, &ChainMessage, &ApplyRet) -> anyhow::Result<()>> = None;

impl<DB> StateManager<DB>
where
    DB: Blockstore,
{
    pub fn new(
        cs: Arc<ChainStore<DB>>,
        chain_config: Arc<ChainConfig>,
    ) -> Result<Self, anyhow::Error> {
        let genesis = cs.genesis();
        let beacon = Arc::new(chain_config.get_beacon_schedule(genesis.timestamp()));

        Ok(Self {
            cs,
            cache: TipsetStateCache::new(),
            beacon,
            chain_config,
            engine: crate::shim::machine::MultiEngine::default(),
        })
    }

    pub fn beacon_schedule(&self) -> Arc<BeaconSchedule<DrandBeacon>> {
        Arc::clone(&self.beacon)
    }

    /// Returns network version for the given epoch.
    pub fn get_network_version(&self, epoch: ChainEpoch) -> NetworkVersion {
        self.chain_config.network_version(epoch)
    }

    pub fn chain_config(&self) -> Arc<ChainConfig> {
        Arc::clone(&self.chain_config)
    }

    /// Gets actor from given [`Cid`], if it exists.
    pub fn get_actor(&self, addr: &Address, state_cid: Cid) -> anyhow::Result<Option<ActorState>> {
        let state = StateTree::new_from_root(self.blockstore_owned(), &state_cid)?;
        state.get_actor(addr)
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

    // This function used to do this: Returns the network name from the init actor
    // state.
    /// Returns the internal, protocol-level network name.
    // TODO: Once we are able to query the init actor state to obtain the network name from the
    // genesis file, this should be removed. It is work in progress here:
    // https://github.com/ChainSafe/forest/pull/2913
    pub fn get_network_name(&self, _st: &Cid) -> Result<String, Error> {
        let name = match &self.chain_config.network {
            crate::networks::NetworkChain::Mainnet => "testnetnet",
            crate::networks::NetworkChain::Calibnet => "calibrationnet",
            crate::networks::NetworkChain::Devnet(name) => name,
        }
        .to_string();
        Ok(name)
    }

    /// Returns true if miner has been slashed or is considered invalid.
    pub fn is_miner_slashed(&self, addr: &Address, state_cid: &Cid) -> anyhow::Result<bool, Error> {
        let actor = self
            .get_actor(&Address::POWER_ACTOR, *state_cid)?
            .ok_or_else(|| Error::State("Power actor address could not be resolved".to_string()))?;

        let spas = power::State::load(self.blockstore(), actor.code, actor.state)?;

        Ok(spas.miner_power(self.blockstore(), &addr.into())?.is_none())
    }

    /// Returns raw work address of a miner given the state root.
    pub fn get_miner_work_addr(
        &self,
        state_cid: Cid,
        addr: &Address,
    ) -> anyhow::Result<Address, Error> {
        let state = StateTree::new_from_root(self.blockstore_owned(), &state_cid)
            .map_err(|e| Error::Other(e.to_string()))?;

        let act = state
            .get_actor(addr)
            .map_err(|e| Error::State(e.to_string()))?
            .ok_or_else(|| Error::State("Miner actor not found".to_string()))?;

        let ms = miner::State::load(self.blockstore(), act.code, act.state)?;

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
            .ok_or_else(|| Error::State("Power actor address could not be resolved".to_string()))?;

        let spas = power::State::load(self.blockstore(), actor.code, actor.state)?;

        let t_pow = spas.total_power();

        if let Some(maddr) = addr {
            let m_pow = spas
                .miner_power(self.blockstore(), &maddr.into())?
                .ok_or_else(|| Error::State(format!("Miner for address {maddr} not found")))?;

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
}

impl<DB> StateManager<DB>
where
    DB: Blockstore + Send + Sync + 'static,
{
    /// Returns the pair of (parent state root, message receipt root). This will
    /// either be cached or will be calculated and fill the cache. Tipset
    /// state for a given tipset is guaranteed not to be computed twice.
    #[instrument(skip(self))]
    pub async fn tipset_state(self: &Arc<Self>, tipset: &Arc<Tipset>) -> anyhow::Result<CidPair> {
        let key = tipset.key();
        self.cache
            .get_or_else(key, || async move {
                let ts_state = self
                    .compute_tipset_state(Arc::clone(tipset), NO_CALLBACK)
                    .await?;
                debug!("Completed tipset state calculation {:?}", tipset.cids());
                Ok(ts_state)
            })
            .await
    }

    #[instrument(skip(self, rand))]
    fn call_raw(
        self: &Arc<Self>,
        msg: &mut Message,
        rand: ChainRand<DB>,
        tipset: &Arc<Tipset>,
    ) -> StateCallResult {
        let bstate = tipset.parent_state();
        let bheight = tipset.epoch();
        let genesis_info = GenesisInfo::from_chain_config(&self.chain_config());
        let mut vm = VM::new(
            ExecutionContext {
                heaviest_tipset: Arc::clone(tipset),
                state_tree_root: *bstate,
                epoch: bheight,
                rand: Box::new(rand),
                base_fee: TokenAmount::zero(),
                circ_supply: genesis_info.get_circulating_supply(
                    bheight,
                    &self.blockstore_owned(),
                    bstate,
                )?,
                chain_config: self.chain_config(),
                chain_index: Arc::clone(&self.chain_store().chain_index),
                timestamp: tipset.min_timestamp(),
            },
            &self.engine,
        )?;

        if msg.gas_limit == 0 {
            msg.gas_limit = 10000000000;
        }

        let actor = self
            .get_actor(&msg.from, *bstate)?
            .ok_or_else(|| Error::Other("Could not get actor".to_string()))?;
        msg.sequence = actor.sequence;
        let apply_ret = vm.apply_implicit_message(msg)?;
        trace!(
            "gas limit {:},gas premium{:?},value {:?}",
            msg.gas_limit,
            msg.gas_premium,
            msg.value
        );
        if let Some(err) = &apply_ret.failure_info() {
            warn!("chain call failed: {:?}", err);
        }

        Ok(InvocResult {
            msg: msg.clone(),
            msg_rct: Some(apply_ret.msg_receipt()),
            error: apply_ret.failure_info(),
        })
    }

    /// runs the given message and returns its result without any persisted
    /// changes.
    pub fn call(
        self: &Arc<Self>,
        message: &mut Message,
        tipset: Option<Arc<Tipset>>,
    ) -> StateCallResult {
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
    ) -> StateCallResult {
        let ts = tipset.unwrap_or_else(|| self.cs.heaviest_tipset());
        let (st, _) = self
            .tipset_state(&ts)
            .await
            .map_err(|_| Error::Other("Could not load tipset state".to_string()))?;
        let chain_rand = self.chain_rand(Arc::clone(&ts));

        // Since we're simulating a future message, pretend we're applying it in the
        // "next" tipset
        let epoch = ts.epoch() + 1;
        let genesis_info = GenesisInfo::from_chain_config(&self.chain_config());
        let mut vm = VM::new(
            ExecutionContext {
                heaviest_tipset: Arc::clone(&ts),
                state_tree_root: st,
                epoch,
                rand: Box::new(chain_rand),
                base_fee: ts.blocks()[0].parent_base_fee().clone(),
                circ_supply: genesis_info.get_circulating_supply(
                    epoch,
                    &self.blockstore_owned(),
                    &st,
                )?,
                chain_config: self.chain_config(),
                chain_index: Arc::clone(&self.chain_store().chain_index),
                timestamp: ts.min_timestamp(),
            },
            &self.engine,
        )?;

        for msg in prior_messages {
            vm.apply_message(msg)?;
        }
        let from_actor = vm
            .get_actor(&message.from())
            .map_err(|e| Error::Other(format!("Could not get actor from state: {e}")))?
            .ok_or_else(|| Error::Other("cant find actor in state tree".to_string()))?;
        message.set_sequence(from_actor.sequence);

        let ret = vm.apply_message(message)?;

        Ok(InvocResult {
            msg: message.message().clone(),
            msg_rct: Some(ret.msg_receipt()),
            error: ret.failure_info(),
        })
    }

    /// Replays the given message and returns the result of executing the
    /// indicated message, assuming it was executed in the indicated tipset.
    pub async fn replay(
        self: &Arc<Self>,
        ts: &Arc<Tipset>,
        mcid: Cid,
    ) -> Result<(Message, ApplyRet), Error> {
        const ERROR_MSG: &str = "replay_halt";

        // This isn't ideal to have, since the execution is synchronous, but this needs
        // to be the case because the state transition has to be in blocking
        // thread to avoid starving executor
        let (m_tx, m_rx) = std::sync::mpsc::channel();
        let (r_tx, r_rx) = std::sync::mpsc::channel();
        let callback = move |cid: &Cid, unsigned: &ChainMessage, apply_ret: &ApplyRet| {
            if *cid == mcid {
                m_tx.send(unsigned.message().clone())?;
                r_tx.send(apply_ret.clone())?;
                anyhow::bail!(ERROR_MSG);
            }
            Ok(())
        };
        let result = self
            .compute_tipset_state(Arc::clone(ts), Some(callback))
            .await;

        if let Err(error_message) = result {
            if error_message.to_string() != ERROR_MSG {
                return Err(Error::Other(format!(
                    "unexpected error during execution : {error_message:}"
                )));
            }
        }

        // Use try_recv here assuming callback execution is synchronous
        let out_mes = m_rx
            .try_recv()
            .map_err(|err| Error::Other(format!("given message not found in tipset: {err}")))?;
        let out_ret = r_rx
            .try_recv()
            .map_err(|err| Error::Other(format!("message did not have a return: {err}")))?;
        Ok((out_mes, out_ret))
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
            .ok_or_else(|| Error::State("Power actor address could not be resolved".to_string()))?;

        let power_state = power::State::load(self.blockstore(), actor.code, actor.state)?;

        let actor = self
            .get_actor(address, *base_tipset.parent_state())?
            .ok_or_else(|| Error::State("Miner actor address could not be resolved".to_string()))?;

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
    #[instrument(skip(self, tipset, callback))]
    pub async fn compute_tipset_state<CB: 'static>(
        self: &Arc<Self>,
        tipset: Arc<Tipset>,
        callback: Option<CB>,
    ) -> Result<CidPair, Error>
    where
        CB: FnMut(&Cid, &ChainMessage, &ApplyRet) -> Result<(), anyhow::Error> + Send,
    {
        let this = Arc::clone(self);
        tokio::task::spawn_blocking(move || this.compute_tipset_state_blocking(tipset, callback))
            .await?
    }

    /// Blocking version of `compute_tipset_state`
    #[tracing::instrument(skip_all)]
    pub fn compute_tipset_state_blocking<CB: 'static>(
        self: &Arc<Self>,
        tipset: Arc<Tipset>,
        callback: Option<CB>,
    ) -> Result<CidPair, Error>
    where
        CB: FnMut(&Cid, &ChainMessage, &ApplyRet) -> Result<(), anyhow::Error> + Send,
    {
        Ok(apply_block_messages(
            self.chain_store().genesis().timestamp(),
            Arc::clone(&self.chain_store().chain_index),
            Arc::clone(&self.chain_config),
            self.beacon_schedule(),
            &self.engine,
            tipset,
            callback,
        )?)
    }

    /// Check if tipset had executed the message, by loading the receipt based
    /// on the index of the message in the block.
    fn tipset_executed_message(
        &self,
        tipset: &Tipset,
        msg_cid: Cid,
        (message_from_address, message_sequence): (&Address, &u64),
    ) -> Result<Option<Receipt>, Error> {
        if tipset.epoch() == 0 {
            return Ok(None);
        }
        // Load parent state.
        let pts = self
            .cs
            .tipset_from_keys(tipset.parents())
            .map_err(|err| Error::Other(err.to_string()))?;
        let messages = self
            .cs
            .messages_for_tipset(&pts)
            .map_err(|err| Error::Other(err.to_string()))?;
        messages
            .iter()
            .enumerate()
            // reverse iteration intentional
            .rev()
            .filter(|(_, s)| {
                &s.from() == message_from_address
            })
            .filter_map(|(index, s)| {
                if s.sequence() == *message_sequence {
                    if s.cid().map(|s|
                        s == msg_cid
                    ).unwrap_or_default() {
                        // When message Cid has been found, get receipt at index.
                        let rct = crate::chain::get_parent_reciept(
                            self.blockstore(),
                            tipset.blocks().first().unwrap(),
                            index,
                        )
                            .map_err(|err| {
                                Error::Other(err.to_string())
                            });
                        return Some(
                           rct
                        );
                    }
                    let error_msg = format!("found message with equal nonce as the one we are looking for (F:{:} n {:}, TS: `Error Converting message to Cid` n{:})", msg_cid, message_sequence, s.sequence());
                    return Some(Err(Error::Other(error_msg)))
                }
                if s.sequence() < *message_sequence {
                    return Some(Ok(None));
                }

                None
            })
            .next()
            .unwrap_or(Ok(None))
    }

    fn check_search(
        &self,
        mut current: Arc<Tipset>,
        (message_from_address, message_cid, message_sequence): (&Address, &Cid, &u64),
    ) -> Result<Option<(Arc<Tipset>, Receipt)>, Error> {
        loop {
            if current.epoch() == 0 {
                return Ok(None);
            }
            let state = StateTree::new_from_root(self.blockstore_owned(), current.parent_state())
                .map_err(|e| Error::State(e.to_string()))?;

            if let Some(actor_state) = state
                .get_actor(message_from_address)
                .map_err(|e| Error::State(e.to_string()))?
            {
                if actor_state.sequence == 0 || actor_state.sequence < *message_sequence {
                    return Ok(None);
                }
            }

            let tipset = self.cs.tipset_from_keys(current.parents()).map_err(|err| {
                Error::Other(format!(
                    "failed to load tipset during msg wait searchback: {err:}"
                ))
            })?;
            let r = self.tipset_executed_message(
                &tipset,
                *message_cid,
                (message_from_address, message_sequence),
            )?;

            if let Some(receipt) = r {
                return Ok(Some((tipset, receipt)));
            }
            current = tipset;
        }
    }

    fn search_back_for_message(
        &self,
        current: Arc<Tipset>,
        params: (&Address, &Cid, &u64),
    ) -> Result<Option<(Arc<Tipset>, Receipt)>, Error> {
        self.check_search(current, params)
    }
    /// Returns a message receipt from a given tipset and message CID.
    pub fn get_receipt(&self, tipset: Arc<Tipset>, msg: Cid) -> Result<Receipt, Error> {
        let m = crate::chain::get_chain_message(self.blockstore(), &msg)
            .map_err(|e| Error::Other(e.to_string()))?;
        let message_var = (&m.from(), &m.sequence());
        let message_receipt = self.tipset_executed_message(&tipset, msg, message_var)?;

        if let Some(receipt) = message_receipt {
            return Ok(receipt);
        }
        let cid = m
            .cid()
            .map_err(|e| Error::Other(format!("Could not convert message to cid {e:?}")))?;
        let message_var = (&m.from(), &cid, &m.sequence());
        let maybe_tuple = self.search_back_for_message(tipset, message_var)?;
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
    ) -> Result<(Option<Arc<Tipset>>, Option<Receipt>), Error> {
        let mut subscriber = self.cs.publisher().subscribe();
        let (sender, mut receiver) = oneshot::channel::<()>();
        let message = crate::chain::get_chain_message(self.blockstore(), &msg_cid)
            .map_err(|err| Error::Other(format!("failed to load message {err:}")))?;

        let message_var = (&message.from(), &message.sequence());
        let current_tipset = self.cs.heaviest_tipset();
        let maybe_message_reciept =
            self.tipset_executed_message(&current_tipset, msg_cid, message_var)?;
        if let Some(r) = maybe_message_reciept {
            return Ok((Some(current_tipset.clone()), Some(r)));
        }

        let mut candidate_tipset: Option<Arc<Tipset>> = None;
        let mut candidate_receipt: Option<Receipt> = None;

        let sm_cloned = Arc::clone(self);
        let cid = message
            .cid()
            .map_err(|e| Error::Other(format!("Could not get cid from message {e:?}")))?;

        let cid_for_task = cid;
        let address_for_task = message.from();
        let sequence_for_task = message.sequence();
        let height_of_head = current_tipset.epoch();
        let task = tokio::task::spawn(async move {
            let back_tuple = sm_cloned.search_back_for_message(
                current_tipset,
                (&address_for_task, &cid_for_task, &sequence_for_task),
            )?;
            sender
                .send(())
                .map_err(|e| Error::Other(format!("Could not send to channel {e:?}")))?;
            Ok::<_, Error>(back_tuple)
        });

        let reverts: Arc<RwLock<HashMap<TipsetKeys, bool>>> = Arc::new(RwLock::new(HashMap::new()));
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

                            let message_var = (&message.from(), &message.sequence());
                            let maybe_receipt =
                                sm_cloned.tipset_executed_message(&tipset, msg_cid, message_var)?;
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
        // TODO this should be a future race. I don't think the task is being cancelled
        // here This seems like it will keep the other task running even though
        // it's unneeded.
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

    /// Returns a BLS public key from provided address
    pub fn get_bls_public_key(
        db: &Arc<DB>,
        addr: &Address,
        state_cid: Cid,
    ) -> Result<[u8; BLS_PUB_LEN], Error> {
        let state = StateTree::new_from_root(Arc::clone(db), &state_cid)
            .map_err(|e| Error::Other(e.to_string()))?;
        let kaddr = resolve_to_key_addr(&state, db, addr)
            .map_err(|e| format!("Failed to resolve key address, error: {e}"))?;

        match kaddr.into_payload() {
            Payload::BLS(key) => Ok(key),
            _ => Err(Error::State(
                "Address must be BLS address to load bls public key".to_owned(),
            )),
        }
    }

    /// Looks up ID [Address] from the state at the given [Tipset].
    pub fn lookup_id(&self, addr: &Address, ts: &Tipset) -> Result<Option<Address>, Error> {
        let state_tree = StateTree::new_from_root(self.blockstore_owned(), ts.parent_state())
            .map_err(|e| e.to_string())?;
        Ok(state_tree
            .lookup_id(addr)
            .map_err(|e| Error::Other(e.to_string()))?
            .map(Address::new_id))
    }

    /// Retrieves market balance in escrow and locked tables.
    pub fn market_balance(
        &self,
        addr: &Address,
        ts: &Tipset,
    ) -> anyhow::Result<MarketBalance, Error> {
        let actor = self
            .get_actor(&Address::MARKET_ACTOR, *ts.parent_state())?
            .ok_or_else(|| {
                Error::State("Market actor address could not be resolved".to_string())
            })?;

        let market_state = market::State::load(self.blockstore(), actor.code, actor.state)?;

        let new_addr = self
            .lookup_id(addr, ts)?
            .ok_or_else(|| Error::State(format!("Failed to resolve address {addr}")))?;

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
            .ok_or_else(|| Error::State("Power actor address could not be resolved".to_string()))?;
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
            .context(format!(
            "couldn't get a tipset at height {} behind heaviest tipset at height {heaviest_epoch}",
            *epochs.end(),
        ))?;

        // lookup tipset parents as we go along, iterating DOWN from `end`
        let tipsets = itertools::unfold(Some(end), |tipset| {
            let child = tipset.take()?;
            // if this has parents, unfold them in the next iteration
            *tipset = self.cs.tipset_from_keys(child.parents()).ok();
            Some(child)
        })
        .take_while(|tipset| tipset.epoch() >= *epochs.start());

        self.validate_tipsets(tipsets)
    }

    pub fn validate_tipsets<T>(self: &Arc<Self>, tipsets: T) -> anyhow::Result<()>
    where
        T: Iterator<Item = Arc<Tipset>> + Send,
    {
        let genesis_timestamp = self.chain_store().genesis().timestamp();
        validate_tipsets(
            genesis_timestamp,
            self.chain_store().chain_index.clone(),
            self.chain_config(),
            self.beacon_schedule(),
            &self.engine,
            tipsets,
        )
    }

    fn chain_rand(&self, tipset: Arc<Tipset>) -> ChainRand<DB> {
        ChainRand::new(
            self.chain_config.clone(),
            tipset,
            self.cs.chain_index.clone(),
            self.beacon.clone(),
        )
    }
}

pub fn validate_tipsets<DB, T>(
    genesis_timestamp: u64,
    chain_index: Arc<ChainIndex<Arc<DB>>>,
    chain_config: Arc<ChainConfig>,
    beacon: Arc<BeaconSchedule<DrandBeacon>>,
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
            let (actual_state, actual_receipt) = apply_block_messages(
                genesis_timestamp,
                chain_index.clone(),
                chain_config.clone(),
                beacon.clone(),
                engine,
                parent,
                NO_CALLBACK,
            )
            .context("couldn't compute tipset state")?;
            let expected_receipt = child.min_ticket_block().message_receipts();
            let expected_state = child.parent_state();
            match (expected_state, expected_receipt) == (&actual_state, &actual_receipt) {
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
pub fn apply_block_messages<DB, CB>(
    genesis_timestamp: u64,
    chain_index: Arc<ChainIndex<Arc<DB>>>,
    chain_config: Arc<ChainConfig>,
    beacon: Arc<BeaconSchedule<DrandBeacon>>,
    engine: &crate::shim::machine::MultiEngine,
    tipset: Arc<Tipset>,
    mut callback: Option<CB>,
) -> Result<CidPair, anyhow::Error>
where
    DB: Blockstore + Send + Sync + 'static,
    CB: FnMut(&Cid, &ChainMessage, &ApplyRet) -> Result<(), anyhow::Error>,
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
        let message_receipts = tipset.min_ticket_block().message_receipts();
        return Ok((*tipset.parent_state(), *message_receipts));
    }

    let _timer = metrics::APPLY_BLOCKS_TIME.start_timer();

    let rand = ChainRand::new(
        Arc::clone(&chain_config),
        Arc::clone(&tipset),
        Arc::clone(&chain_index),
        beacon,
    );

    let genesis_info = GenesisInfo::from_chain_config(&chain_config);
    let create_vm = |state_root: Cid, epoch, timestamp| {
        let circulating_supply =
            genesis_info.get_circulating_supply(epoch, &chain_index.db, &state_root)?;
        VM::new(
            ExecutionContext {
                heaviest_tipset: Arc::clone(&tipset),
                state_tree_root: state_root,
                epoch,
                rand: Box::new(rand.clone()),
                base_fee: tipset.min_ticket_block().parent_base_fee().clone(),
                circ_supply: circulating_supply,
                chain_config: Arc::clone(&chain_config),
                chain_index: Arc::clone(&chain_index),
                timestamp,
            },
            engine,
        )
    };

    let mut parent_state = *tipset.parent_state();

    let parent_epoch = Tipset::load_required(&chain_index.db, tipset.parents())?.epoch();
    let epoch = tipset.epoch();

    for epoch_i in parent_epoch..epoch {
        if epoch_i > parent_epoch {
            // step 2: running cron for any null-tipsets
            let timestamp = genesis_timestamp + ((EPOCH_DURATION_SECONDS * epoch_i) as u64);
            let mut vm = create_vm(parent_state, epoch_i, timestamp)?;
            // run cron for null rounds if any
            if let Err(e) = vm.run_cron(epoch_i, callback.as_mut()) {
                error!("Beginning of epoch cron failed to run: {}", e);
            }

            parent_state = vm.flush()?;
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

    let mut vm = create_vm(parent_state, epoch, tipset.min_timestamp())?;

    // step 4: apply tipset messages
    let receipts = vm.apply_block_messages(&block_messages, epoch, callback)?;

    // step 5: construct receipt root from receipts and flush the state-tree
    let receipt_root = Amt::new_from_iter(&chain_index.db, receipts)?;
    let state_root = vm.flush()?;

    Ok((state_root, receipt_root))
}
