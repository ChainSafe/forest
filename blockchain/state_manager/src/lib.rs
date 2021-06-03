// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#[macro_use]
extern crate lazy_static;

mod chain_rand;
mod errors;
mod utils;
mod vm_circ_supply;

pub use self::errors::*;
use actor::*;
use address::{Address, BLSPublicKey, Payload, Protocol, BLS_PUB_LEN};
use async_log::span;
use async_std::{sync::RwLock, task};
use beacon::{Beacon, BeaconEntry, BeaconSchedule, IGNORE_DRAND_VAR};
use blockstore::{BlockStore, BufferedBlockStore};
use chain::{draw_randomness, ChainStore, HeadChange};
use chain_rand::ChainRand;
use cid::Cid;
use clock::ChainEpoch;
use encoding::Cbor;
use fil_types::{verifier::ProofVerifier, NetworkVersion, Randomness, SectorInfo, SectorSize};
use forest_blocks::{BlockHeader, Tipset, TipsetKeys};
use forest_crypto::DomainSeparationTag;
use futures::{channel::oneshot, select, FutureExt};
use interpreter::{
    resolve_to_key_addr, ApplyRet, BlockMessages, CircSupplyCalc, LookbackStateGetter, Rand, VM,
};
use ipld_amt::Amt;
use log::{debug, info, trace, warn};
use message::{
    message_receipt, unsigned_message, ChainMessage, Message, MessageReceipt, UnsignedMessage,
};
use networks::get_network_version_default;
use num_bigint::{bigint_ser, BigInt};
use num_traits::identities::Zero;
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};
use state_tree::StateTree;
use std::collections::HashMap;
use std::error::Error as StdError;
use std::marker::PhantomData;
use std::sync::Arc;
use tokio::sync::broadcast::{error::RecvError, Receiver as Subscriber, Sender as Publisher};
use vm::{ActorState, TokenAmount};
use vm_circ_supply::GenesisInfo;

/// Intermediary for retrieving state objects and updating actor states.
type CidPair = (Cid, Cid);

/// Type to represent invocation of state call results.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct InvocResult {
    #[serde(with = "unsigned_message::json")]
    pub msg: UnsignedMessage,
    #[serde(with = "message_receipt::json::opt")]
    pub msg_rct: Option<MessageReceipt>,
    pub error: Option<String>,
}

/// An alias Result that represents an InvocResult and an Error.
type StateCallResult = Result<InvocResult, Error>;

/// External format for returning market balance from state.
#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct MarketBalance {
    #[serde(with = "bigint_ser")]
    escrow: BigInt,
    #[serde(with = "bigint_ser")]
    locked: BigInt,
}

/// State manager handles all interactions with the internal Filecoin actors state.
/// This encapsulates the [ChainStore] functionality, which only handles chain data, to
/// allow for interactions with the underlying state of the chain. The state manager not only
/// allows interfacing with state, but also is used when performing state transitions.
pub struct StateManager<DB> {
    cs: Arc<ChainStore<DB>>,

    /// This is a cache which indexes tipsets to their calculated state.
    /// The calculated state is wrapped in a mutex to avoid duplicate computation
    /// of the state/receipt root.
    cache: RwLock<HashMap<TipsetKeys, Arc<RwLock<Option<CidPair>>>>>,
    publisher: Option<Publisher<HeadChange>>,
    genesis_info: GenesisInfo,
}

impl<DB> StateManager<DB>
where
    DB: BlockStore + Send + Sync + 'static,
{
    pub fn new(cs: Arc<ChainStore<DB>>) -> Self {
        Self {
            cs,
            cache: RwLock::new(HashMap::new()),
            publisher: None,
            genesis_info: GenesisInfo::default(),
        }
    }

    /// Creates a constructor that passes in a HeadChange publisher.
    pub fn new_with_publisher(cs: Arc<ChainStore<DB>>, chain_subs: Publisher<HeadChange>) -> Self {
        Self {
            cs,
            cache: RwLock::new(HashMap::new()),
            publisher: Some(chain_subs),
            genesis_info: GenesisInfo::default(),
        }
    }

    /// Returns network version for the given epoch.
    pub fn get_network_version(&self, epoch: ChainEpoch) -> NetworkVersion {
        get_network_version_default(epoch)
    }

    /// Gets actor from given [Cid], if it exists.
    pub fn get_actor(&self, addr: &Address, state_cid: &Cid) -> Result<Option<ActorState>, Error> {
        let state = StateTree::new_from_root(self.blockstore(), state_cid)?;
        Ok(state.get_actor(addr)?)
    }

    /// Returns the cloned [Arc] of the state manager's [BlockStore].
    pub fn blockstore_cloned(&self) -> Arc<DB> {
        self.cs.blockstore_cloned()
    }

    /// Returns a reference to the state manager's [BlockStore].
    pub fn blockstore(&self) -> &DB {
        self.cs.blockstore()
    }

    /// Returns reference to the state manager's [ChainStore].
    pub fn chain_store(&self) -> &Arc<ChainStore<DB>> {
        &self.cs
    }

    /// Returns the network name from the init actor state.
    pub fn get_network_name(&self, st: &Cid) -> Result<String, Error> {
        let init_act = self
            .get_actor(actor::init::ADDRESS, st)?
            .ok_or_else(|| Error::State("Init actor address could not be resolved".to_string()))?;

        let state = init::State::load(self.blockstore(), &init_act)?;
        Ok(state.into_network_name())
    }

    /// Returns true if miner has been slashed or is considered invalid.
    pub fn is_miner_slashed(&self, addr: &Address, state_cid: &Cid) -> Result<bool, Error> {
        let actor = self
            .get_actor(actor::power::ADDRESS, state_cid)?
            .ok_or_else(|| Error::State("Power actor address could not be resolved".to_string()))?;

        let spas = power::State::load(self.blockstore(), &actor)?;

        Ok(spas.miner_power(self.blockstore(), addr)?.is_none())
    }

    /// Returns raw work address of a miner given the state root.
    pub fn get_miner_work_addr(&self, state_cid: &Cid, addr: &Address) -> Result<Address, Error> {
        let state = StateTree::new_from_root(self.blockstore(), state_cid)?;

        let act = state
            .get_actor(addr)?
            .ok_or_else(|| Error::State("Miner actor not found".to_string()))?;

        let ms = miner::State::load(self.blockstore(), &act)?;

        let info = ms.info(self.blockstore()).map_err(|e| e.to_string())?;

        let addr = resolve_to_key_addr(&state, self.blockstore(), &info.worker())
            .map_err(|e| Error::Other(format!("Failed to resolve key address; error: {}", e)))?;
        Ok(addr)
    }

    /// Returns specified actor's claimed power and total network power as a tuple.
    pub fn get_power(
        &self,
        state_cid: &Cid,
        addr: Option<&Address>,
    ) -> Result<Option<(power::Claim, power::Claim)>, Error> {
        let actor = self
            .get_actor(actor::power::ADDRESS, state_cid)?
            .ok_or_else(|| Error::State("Power actor address could not be resolved".to_string()))?;

        let spas = power::State::load(self.blockstore(), &actor)?;

        let t_pow = spas.total_power();

        if let Some(maddr) = addr {
            let m_pow = spas
                .miner_power(self.blockstore(), maddr)?
                .ok_or_else(|| Error::State(format!("Miner for address {} not found", maddr)))?;

            let min_pow =
                spas.miner_nominal_power_meets_consensus_minimum(self.blockstore(), maddr)?;
            if min_pow {
                return Ok(Some((m_pow, t_pow)));
            }
        }

        Ok(None)
    }

    /// Subscribes to the [HeadChange]s observed by the state manager.
    pub fn get_subscriber(&self) -> Option<Subscriber<HeadChange>> {
        self.publisher.as_ref().map(|p| p.subscribe())
    }

    /// Performs the state transition for the tipset and applies all unique messages in all blocks.
    /// This function returns the state root and receipt root of the transition.
    #[allow(clippy::too_many_arguments)]
    pub fn apply_blocks<R, V, CB>(
        self: &Arc<Self>,
        parent_epoch: ChainEpoch,
        p_state: &Cid,
        messages: &[BlockMessages],
        epoch: ChainEpoch,
        rand: &R,
        base_fee: BigInt,
        callback: Option<CB>,
        tipset: &Arc<Tipset>,
    ) -> Result<CidPair, Box<dyn StdError>>
    where
        R: Rand,
        V: ProofVerifier,
        CB: FnMut(&Cid, &ChainMessage, &ApplyRet) -> Result<(), String>,
    {
        let db = self.blockstore_cloned();
        let mut buf_store = Arc::new(BufferedBlockStore::new(db.as_ref()));
        let store = buf_store.as_ref();
        let lb_wrapper = SMLookbackWrapper {
            sm: self,
            store,
            tipset,
            verifier: PhantomData::<V>::default(),
        };

        let mut vm = VM::<_, _, _, _, _, V>::new(
            p_state,
            store,
            epoch,
            rand,
            base_fee,
            get_network_version_default,
            &self.genesis_info,
            &lb_wrapper,
        )?;

        // Apply tipset messages
        let receipts =
            vm.apply_block_messages(messages, parent_epoch, epoch, buf_store.clone(), callback)?;

        // Construct receipt root from receipts
        let rect_root = Amt::new_from_iter(self.blockstore(), receipts)?;
        // Flush changes to blockstore
        let state_root = vm.flush()?;
        // Persist changes connected to root
        Arc::get_mut(&mut buf_store)
            .expect("failed getting store reference")
            .flush(&state_root)
            .expect("buffered blockstore flush failed");

        Ok((state_root, rect_root))
    }

    /// Returns the pair of (parent state root, message receipt root). This will either be cached
    /// or will be calculated and fill the cache. Tipset state for a given tipset is guaranteed
    /// not to be computed twice.
    pub async fn tipset_state<V>(
        self: &Arc<Self>,
        tipset: &Arc<Tipset>,
    ) -> Result<CidPair, Box<dyn StdError>>
    where
        V: ProofVerifier,
    {
        span!("tipset_state", {
            // Get entry in cache, if it exists.
            // Arc is cloned here to avoid holding the entire cache lock until function ends.
            // (tasks should be able to compute different tipset state's in parallel)
            //
            // In the case of task `A` computing the same tipset as task `B`, `A` will hold the
            // mutex until the value is updated, which task `B` will await.
            //
            // If two tasks are computing different tipset states, they will only block computation
            // when accessing/initializing the entry in cache, not during the whole tipset calc.
            let cache_entry: Arc<_> = self
                .cache
                .write()
                .await
                .entry(tipset.key().clone())
                .or_default()
                // Clone Arc to drop lock of cache
                .clone();

            // Try to lock cache entry to ensure task is first to compute state.
            // If another task has the lock, it will overwrite the state before releasing lock.
            let mut entry_lock = cache_entry.write().await;
            if let Some(ref entry) = *entry_lock {
                // Entry had successfully populated state, return Cid and drop lock
                trace!("hit cache for tipset {:?}", tipset.cids());
                return Ok(*entry);
            }

            // Entry does not have state computed yet, this task will fill entry if successful.
            debug!("calculating tipset state {:?}", tipset.cids());

            let cid_pair = if tipset.epoch() == 0 {
                // NB: This is here because the process that executes blocks requires that the
                // block miner reference a valid miner in the state tree. Unless we create some
                // magical genesis miner, this won't work properly, so we short circuit here
                // This avoids the question of 'who gets paid the genesis block reward'
                let message_receipts = tipset
                    .blocks()
                    .first()
                    .ok_or_else(|| Error::Other("Could not get message receipts".to_string()))?;

                (*tipset.parent_state(), *message_receipts.message_receipts())
            } else {
                // generic constants are not implemented yet this is a lowcost method for now
                let no_func = None::<fn(&Cid, &ChainMessage, &ApplyRet) -> Result<(), String>>;
                self.compute_tipset_state::<V, _>(&tipset, no_func).await?
            };

            // Fill entry with calculated cid pair
            *entry_lock = Some(cid_pair);
            Ok(cid_pair)
        })
    }

    fn call_raw<V>(
        self: &Arc<Self>,
        msg: &mut UnsignedMessage,
        rand: &ChainRand<DB>,
        tipset: &Arc<Tipset>,
    ) -> StateCallResult
    where
        V: ProofVerifier,
    {
        span!("state_call_raw", {
            let bstate = tipset.parent_state();
            let bheight = tipset.epoch();
            let block_store = self.blockstore();

            let buf_store = BufferedBlockStore::new(block_store);
            let lb_wrapper = SMLookbackWrapper {
                sm: self,
                store: &buf_store,
                tipset,
                verifier: PhantomData::<V>::default(),
            };
            let mut vm = VM::<_, _, _, _, _, V>::new(
                bstate,
                &buf_store,
                bheight,
                rand,
                0.into(),
                get_network_version_default,
                &self.genesis_info,
                &lb_wrapper,
            )?;

            if msg.gas_limit() == 0 {
                msg.set_gas_limit(10000000000)
            }

            let actor = self
                .get_actor(msg.from(), bstate)?
                .ok_or_else(|| Error::Other("Could not get actor".to_string()))?;
            msg.set_sequence(actor.sequence);
            let apply_ret = vm.apply_implicit_message(msg);
            trace!(
                "gas limit {:},gas premium{:?},value {:?}",
                msg.gas_limit(),
                msg.gas_premium(),
                msg.value()
            );
            if let Some(err) = &apply_ret.act_error {
                warn!("chain call failed: {:?}", err);
            }

            Ok(InvocResult {
                msg: msg.clone(),
                msg_rct: Some(apply_ret.msg_receipt.clone()),
                error: apply_ret.act_error.map(|e| e.to_string()),
            })
        })
    }

    /// runs the given message and returns its result without any persisted changes.
    pub async fn call<V>(
        self: &Arc<Self>,
        message: &mut UnsignedMessage,
        tipset: Option<Arc<Tipset>>,
    ) -> StateCallResult
    where
        V: ProofVerifier,
    {
        let ts = if let Some(t_set) = tipset {
            t_set
        } else {
            self.cs
                .heaviest_tipset()
                .await
                .ok_or_else(|| Error::Other("No heaviest tipset".to_string()))?
        };
        let chain_rand = ChainRand::new(ts.key().to_owned(), self.cs.clone());
        self.call_raw::<V>(message, &chain_rand, &ts)
    }

    /// Computes message on the given [Tipset] state, after applying other messages and returns
    /// the values computed in the VM.
    pub async fn call_with_gas<V>(
        self: &Arc<Self>,
        message: &mut ChainMessage,
        prior_messages: &[ChainMessage],
        tipset: Option<Arc<Tipset>>,
    ) -> StateCallResult
    where
        V: ProofVerifier,
    {
        let ts = if let Some(t_set) = tipset {
            t_set
        } else {
            self.cs
                .heaviest_tipset()
                .await
                .ok_or_else(|| Error::Other("No heaviest tipset".to_string()))?
        };
        let (st, _) = self
            .tipset_state::<V>(&ts)
            .await
            .map_err(|_| Error::Other("Could not load tipset state".to_string()))?;
        let chain_rand = ChainRand::new(ts.key().to_owned(), self.cs.clone());

        // TODO investigate: this doesn't use a buffered store in any way, and can lead to
        // state bloat potentially?
        let lb_wrapper = SMLookbackWrapper {
            sm: self,
            store: self.blockstore(),
            tipset: &ts,
            verifier: PhantomData::<V>::default(),
        };
        let mut vm = VM::<_, _, _, _, _, V>::new(
            &st,
            self.blockstore(),
            ts.epoch() + 1,
            &chain_rand,
            ts.blocks()[0].parent_base_fee().clone(),
            get_network_version_default,
            &self.genesis_info,
            &lb_wrapper,
        )?;

        for msg in prior_messages {
            vm.apply_message(&msg)?;
        }
        let from_actor = vm
            .state()
            .get_actor(message.from())
            .map_err(|e| Error::Other(format!("Could not get actor from state: {}", e)))?
            .ok_or_else(|| Error::Other("cant find actor in state tree".to_string()))?;
        message.set_sequence(from_actor.sequence);

        let ret = vm.apply_message(&message)?;

        Ok(InvocResult {
            msg: message.message().clone(),
            msg_rct: Some(ret.msg_receipt.clone()),
            error: ret.act_error.map(|e| e.to_string()),
        })
    }

    /// Replays the given message and returns the result of executing the indicated message,
    /// assuming it was executed in the indicated tipset.
    pub async fn replay<V>(
        self: &Arc<Self>,
        ts: &Arc<Tipset>,
        mcid: Cid,
    ) -> Result<(UnsignedMessage, ApplyRet), Error>
    where
        V: ProofVerifier,
    {
        // This isn't ideal to have, since the execution is syncronous, but this needs to be the
        // case because the state transition has to be in blocking thread to avoid starving executor
        let outm: OnceCell<UnsignedMessage> = Default::default();
        let outr: OnceCell<ApplyRet> = Default::default();
        let m_clone = outm.clone();
        let r_clone = outr.clone();
        let callback = move |cid: &Cid, unsigned: &ChainMessage, apply_ret: &ApplyRet| {
            if *cid == mcid {
                let _ = m_clone.set(unsigned.message().clone());
                let _ = r_clone.set(apply_ret.clone());
                return Err("halt".to_string());
            }

            Ok(())
        };
        let result = self.compute_tipset_state::<V, _>(&ts, Some(callback)).await;

        if let Err(error_message) = result {
            if error_message.to_string() != "halt" {
                return Err(Error::Other(format!(
                    "unexpected error during execution : {:}",
                    error_message
                )));
            }
        }

        let out_mes = outm
            .into_inner()
            .ok_or_else(|| Error::Other("given message not found in tipset".to_string()))?;
        let out_ret = outr
            .into_inner()
            .ok_or_else(|| Error::Other("message did not have a return".to_string()))?;
        Ok((out_mes, out_ret))
    }

    /// Gets lookback tipset for block validations.
    pub async fn get_lookback_tipset_for_round<V>(
        self: &Arc<Self>,
        tipset: Arc<Tipset>,
        round: ChainEpoch,
    ) -> Result<(Arc<Tipset>, Cid), Error>
    where
        V: ProofVerifier,
    {
        let mut lbr: ChainEpoch = ChainEpoch::from(0);
        let version = get_network_version_default(round);
        let lb = if version <= NetworkVersion::V3 {
            ChainEpoch::from(10)
        } else {
            CHAIN_FINALITY
        };

        if round > lb {
            lbr = round - lb
        }

        // More null blocks than lookback
        if lbr >= tipset.epoch() {
            let (st, _) = self
                .tipset_state::<V>(&tipset)
                .await
                .map_err(|e| Error::Other(format!("Could execute tipset_state {:?}", e)))?;
            return Ok((tipset, st));
        }

        let next_ts = self
            .cs
            .tipset_by_height(lbr + 1, tipset.clone(), false)
            .await
            .map_err(|e| Error::Other(format!("Could not get tipset by height {:?}", e)))?;
        if lbr > next_ts.epoch() {
            return Err(Error::Other(format!(
                "failed to find non-null tipset {:?} {} which is known to exist, found {:?} {}",
                tipset.key(),
                tipset.epoch(),
                next_ts.key(),
                next_ts.epoch()
            )));
        }
        let lbts = self
            .cs
            .tipset_from_keys(next_ts.parents())
            .await
            .map_err(|e| Error::Other(format!("Could not get tipset from keys {:?}", e)))?;
        Ok((lbts, *next_ts.parent_state()))
    }

    /// Checks the eligibility of the miner. This is used in the validation that a block's miner
    /// has the requirements to mine a block.
    pub fn eligible_to_mine(
        self: &Arc<Self>,
        address: &Address,
        base_tipset: &Tipset,
        lookback_tipset: &Tipset,
    ) -> Result<bool, Error> {
        let hmp = self.miner_has_min_power(address, lookback_tipset)?;
        let version = get_network_version_default(base_tipset.epoch());

        if version <= NetworkVersion::V3 {
            return Ok(hmp);
        }

        if !hmp {
            return Ok(false);
        }

        let actor = self
            .get_actor(actor::power::ADDRESS, base_tipset.parent_state())?
            .ok_or_else(|| Error::State("Power actor address could not be resolved".to_string()))?;

        let power_state = power::State::load(self.blockstore(), &actor)?;

        let actor = self
            .get_actor(address, base_tipset.parent_state())?
            .ok_or_else(|| Error::State("Power actor address could not be resolved".to_string()))?;

        let miner_state = miner::State::load(self.blockstore(), &actor)?;

        // Non-empty power claim.
        let claim = power_state
            .miner_power(self.blockstore(), &address)?
            .ok_or_else(|| Error::Other("Could not get claim".to_string()))?;
        if claim.quality_adj_power <= BigInt::zero() {
            return Ok(false);
        }

        // No fee debt.
        if !miner_state.fee_debt().is_zero() {
            return Ok(false);
        }

        // No active consensus faults.
        if base_tipset.epoch() <= miner_state.info(self.blockstore())?.consensus_fault_elapsed {
            return Ok(false);
        }

        Ok(true)
    }

    /// Get's a miner's base info from state, based on the address provided.
    pub async fn miner_get_base_info<V: ProofVerifier, B: Beacon>(
        self: &Arc<Self>,
        beacon: &BeaconSchedule<B>,
        key: &TipsetKeys,
        round: ChainEpoch,
        address: Address,
    ) -> Result<Option<MiningBaseInfo>, Box<dyn StdError>> {
        let tipset = self.cs.tipset_from_keys(key).await?;
        let prev = match self.cs.latest_beacon_entry(&tipset).await {
            Ok(prev) => prev,
            Err(err) => {
                if std::env::var(IGNORE_DRAND_VAR)
                    .map(|e| e != "1")
                    .unwrap_or(true)
                {
                    return Err(Box::from(format!(
                        "failed to get latest beacon entry: {:?}",
                        err
                    )));
                }
                beacon::BeaconEntry::default()
            }
        };
        let entries = beacon
            .beacon_entries_for_block(round, tipset.epoch(), &prev)
            .await?;
        let rbase = entries.iter().last().unwrap_or(&prev);
        let (lbts, lbst) = self
            .get_lookback_tipset_for_round::<V>(tipset.clone(), round)
            .await?;

        let actor = self
            .get_actor(&address, &lbst)?
            .ok_or_else(|| Error::State("Power actor address could not be resolved".to_string()))?;
        let miner_state = miner::State::load(self.blockstore(), &actor)?;

        let buf = address.marshal_cbor()?;
        let prand = draw_randomness(
            rbase.data(),
            DomainSeparationTag::WinningPoStChallengeSeed,
            round,
            &buf,
        )?;

        let nv = get_network_version_default(tipset.epoch());
        let sectors =
            self.get_sectors_for_winning_post::<V>(&lbst, nv, &address, Randomness(prand))?;

        if sectors.is_empty() {
            return Ok(None);
        }

        let (mpow, tpow) = self
            .get_power(&lbst, Some(&address))?
            .ok_or_else(|| Error::State(format!("failed to load power for address {}", address)))?;

        let info = miner_state.info(self.blockstore())?;

        let (st, _) = self.tipset_state::<V>(&lbts).await?;
        let state = StateTree::new_from_root(self.blockstore(), &st)?;

        let worker_key = resolve_to_key_addr(&state, self.blockstore(), &info.worker())?;

        let eligible = self.eligible_to_mine(&address, &tipset.as_ref(), &lbts)?;

        Ok(Some(MiningBaseInfo {
            miner_power: Some(mpow.quality_adj_power),
            network_power: Some(tpow.quality_adj_power),
            sectors,
            worker_key,
            sector_size: info.sector_size(),
            prev_beacon_entry: prev,
            beacon_entries: entries,
            eligible_for_mining: eligible,
        }))
    }

    /// Performs a state transition, and returns the state and receipt root of the transition.
    pub async fn compute_tipset_state<V, CB: 'static>(
        self: &Arc<Self>,
        tipset: &Arc<Tipset>,
        callback: Option<CB>,
    ) -> Result<CidPair, Error>
    where
        V: ProofVerifier,
        CB: FnMut(&Cid, &ChainMessage, &ApplyRet) -> Result<(), String> + Send,
    {
        span!("compute_tipset_state", {
            let block_headers = tipset.blocks();
            let first_block = block_headers
                .first()
                .ok_or_else(|| Error::Other("Empty tipset in compute_tipset_state".to_string()))?;

            let check_for_duplicates = |s: &BlockHeader| {
                block_headers
                    .iter()
                    .filter(|val| val.miner_address() == s.miner_address())
                    .take(2)
                    .count()
            };
            if let Some(a) = block_headers.iter().find(|s| check_for_duplicates(s) > 1) {
                // Duplicate Miner found
                return Err(Error::Other(format!("duplicate miner in a tipset ({})", a)));
            }

            let parent_epoch = if first_block.epoch() > 0 {
                let parent_cid = first_block
                    .parents()
                    .cids()
                    .get(0)
                    .ok_or_else(|| Error::Other("block must have parents".to_string()))?;
                let parent: BlockHeader = self
                    .blockstore()
                    .get(parent_cid)
                    .map_err(|e| Error::Other(e.to_string()))?
                    .ok_or_else(|| {
                        format!("Could not find parent block with cid {}", parent_cid)
                    })?;
                parent.epoch()
            } else {
                Default::default()
            };

            let tipset_keys =
                TipsetKeys::new(block_headers.iter().map(|s| s.cid()).cloned().collect());
            let chain_rand = ChainRand::new(tipset_keys, self.cs.clone());
            let base_fee = first_block.parent_base_fee().clone();

            let blocks = self
                .chain_store()
                .block_msgs_for_tipset(tipset)
                .map_err(|e| Error::Other(e.to_string()))?;

            let sm = self.clone();
            let sr = *first_block.state_root();
            let epoch = first_block.epoch();
            let ts_cloned = tipset.clone();
            task::spawn_blocking(move || {
                sm.apply_blocks::<_, V, _>(
                    parent_epoch,
                    &sr,
                    &blocks,
                    epoch,
                    &chain_rand,
                    base_fee,
                    callback,
                    &ts_cloned,
                )
                .map_err(|e| Error::Other(e.to_string()))
            })
            .await
        })
    }

    /// Check if tipset had executed the message, by loading the receipt based on the index of
    /// the message in the block.
    async fn tipset_executed_message(
        &self,
        tipset: &Tipset,
        msg_cid: &Cid,
        (message_from_address, message_sequence): (&Address, &u64),
    ) -> Result<Option<MessageReceipt>, Error> {
        if tipset.epoch() == 0 {
            return Ok(None);
        }
        // Load parent state.
        let pts = self
            .cs
            .tipset_from_keys(tipset.parents())
            .await
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
                s.from() == message_from_address
            })
            .filter_map(|(index, s)| {
                if s.sequence() == *message_sequence {
                    if s.cid().map(|s|
                        &s == msg_cid
                    ).unwrap_or_default() {
                        // When message Cid has been found, get receipt at index.
                        let rct = chain::get_parent_reciept(
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

    async fn check_search(
        &self,
        current: &Tipset,
        (message_from_address, message_cid, message_sequence): (&Address, &Cid, &u64),
    ) -> Result<Option<(Arc<Tipset>, MessageReceipt)>, Result<Arc<Tipset>, Error>> {
        if current.epoch() == 0 {
            return Ok(None);
        }
        let state = StateTree::new_from_root(self.blockstore(), current.parent_state())
            .map_err(|e| Err(Error::State(e.to_string())))?;

        if let Some(actor_state) = state
            .get_actor(message_from_address)
            .map_err(|e| Err(Error::State(e.to_string())))?
        {
            if actor_state.sequence == 0 || actor_state.sequence < *message_sequence {
                return Ok(None);
            }
        }

        let tipset = self
            .cs
            .tipset_from_keys(current.parents())
            .await
            .map_err(|err| {
                Err(Error::Other(format!(
                    "failed to load tipset during msg wait searchback: {:}",
                    err
                )))
            })?;
        let r = self
            .tipset_executed_message(
                &tipset,
                message_cid,
                (message_from_address, message_sequence),
            )
            .await
            .map_err(Err)?;

        if let Some(receipt) = r {
            Ok(Some((tipset, receipt)))
        } else {
            Err(Ok(tipset))
        }
    }

    async fn search_back_for_message(
        &self,
        current: &Tipset,
        params: (&Address, &Cid, &u64),
    ) -> Result<Option<(Arc<Tipset>, MessageReceipt)>, Error> {
        let mut ts: Arc<Tipset> = match self.check_search(current, params).await {
            Ok(res) => return Ok(res),
            Err(e) => e?,
        };

        // Loops until message is found, genesis is hit, or an error is encountered
        loop {
            ts = match self.check_search(&ts, params).await {
                Ok(res) => return Ok(res),
                Err(e) => e?,
            };
        }
    }
    /// Returns a message receipt from a given tipset and message cid.
    pub async fn get_receipt(&self, tipset: &Tipset, msg: &Cid) -> Result<MessageReceipt, Error> {
        let m = chain::get_chain_message(self.blockstore(), msg)
            .map_err(|e| Error::Other(e.to_string()))?;
        let message_var = (m.from(), &m.sequence());
        let message_receipt = self
            .tipset_executed_message(tipset, msg, message_var)
            .await?;

        if let Some(receipt) = message_receipt {
            return Ok(receipt);
        }
        let cid = m
            .cid()
            .map_err(|e| Error::Other(format!("Could not convert message to cid {:?}", e)))?;
        let message_var = (m.from(), &cid, &m.sequence());
        let maybe_tuple = self.search_back_for_message(tipset, message_var).await?;
        let message_receipt = maybe_tuple
            .ok_or_else(|| {
                Error::Other("Could not get receipt from search back message".to_string())
            })?
            .1;
        Ok(message_receipt)
    }

    /// WaitForMessage blocks until a message appears on chain. It looks backwards in the
    /// chain to see if this has already happened. It guarantees that the message has been on chain
    /// for at least confidence epochs without being reverted before returning.
    pub async fn wait_for_message(
        self: &Arc<Self>,
        msg_cid: Cid,
        confidence: i64,
    ) -> Result<(Option<Arc<Tipset>>, Option<MessageReceipt>), Error>
    where
        DB: BlockStore + Send + Sync + 'static,
    {
        let mut subscriber = self.cs.publisher().subscribe();
        let (sender, mut receiver) = oneshot::channel::<()>();
        let message = chain::get_chain_message(self.blockstore(), &msg_cid)
            .map_err(|err| Error::Other(format!("failed to load message {:}", err)))?;

        let message_var = (message.from(), &message.sequence());
        let current_tipset = self.cs.heaviest_tipset().await.unwrap();
        let maybe_message_reciept = self
            .tipset_executed_message(&current_tipset, &msg_cid, message_var)
            .await?;
        if let Some(r) = maybe_message_reciept {
            return Ok((Some(current_tipset.clone()), Some(r)));
        }

        let mut candidate_tipset: Option<Arc<Tipset>> = None;
        let mut candidate_receipt: Option<MessageReceipt> = None;

        let sm_cloned = self.clone();
        let cid = message
            .cid()
            .map_err(|e| Error::Other(format!("Could not get cid from message {:?}", e)))?;

        let cid_for_task = cid;
        let address_for_task = *message.from();
        let sequence_for_task = message.sequence();
        let height_of_head = current_tipset.epoch();
        let task = task::spawn(async move {
            let back_tuple = sm_cloned
                .search_back_for_message(
                    &current_tipset,
                    (&address_for_task, &cid_for_task, &sequence_for_task),
                )
                .await?;
            sender
                .send(())
                .map_err(|e| Error::Other(format!("Could not send to channel {:?}", e)))?;
            Ok::<_, Error>(back_tuple)
        });

        let reverts: Arc<RwLock<HashMap<TipsetKeys, bool>>> = Arc::new(RwLock::new(HashMap::new()));
        let block_revert = reverts.clone();
        let sm_cloned = self.clone();

        // Wait for message to be included in head change.
        let mut subscriber_poll = task::spawn::<
            _,
            Result<(Option<Arc<Tipset>>, Option<MessageReceipt>), Error>,
        >(async move {
            loop {
                match subscriber.recv().await {
                    Ok(subscriber) => match subscriber {
                        HeadChange::Revert(_tipset) => {
                            if candidate_tipset.is_some() {
                                candidate_tipset = None;
                                candidate_receipt = None;
                            }
                        }
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

                            let message_var = (message.from(), &message.sequence());
                            let maybe_receipt = sm_cloned
                                .tipset_executed_message(&tipset, &msg_cid, message_var)
                                .await?;
                            if let Some(receipt) = maybe_receipt {
                                if confidence == 0 {
                                    return Ok((Some(tipset), Some(receipt)));
                                }
                                candidate_tipset = Some(tipset);
                                candidate_receipt = Some(receipt)
                            }
                        }
                        _ => (),
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
        let mut search_back_poll = task::spawn::<_, Result<_, Error>>(async move {
            let back_tuple = task.await?;
            if let Some((back_tipset, back_receipt)) = back_tuple {
                let should_revert = *reverts
                    .read()
                    .await
                    .get(back_tipset.key())
                    .unwrap_or(&false);
                let larger_height_of_head = height_of_head >= back_tipset.epoch() + confidence;
                if !should_revert && larger_height_of_head {
                    return Ok((Some(back_tipset), Some(back_receipt)));
                }
                return Ok((None, None));
            }
            Ok((None, None))
        })
        .fuse();

        // Await on first future to finish.
        // TODO this should be a future race. I don't think the task is being cancelled here
        // This seems like it will keep the other task running even though it's unneeded.
        loop {
            select! {
                res = subscriber_poll => {
                    return res;
                }
                res = search_back_poll => {
                    if let Ok((Some(ts), Some(rct))) = res {
                        return Ok((Some(ts), Some(rct)));
                    }
                }
            }
        }
    }

    /// Returns a bls public key from provided address
    pub fn get_bls_public_key(
        db: &DB,
        addr: &Address,
        state_cid: &Cid,
    ) -> Result<[u8; BLS_PUB_LEN], Error> {
        let state = StateTree::new_from_root(db, state_cid)?;
        let kaddr = resolve_to_key_addr(&state, db, addr)
            .map_err(|e| format!("Failed to resolve key address, error: {}", e))?;

        match kaddr.into_payload() {
            Payload::BLS(BLSPublicKey(key)) => Ok(key),
            _ => Err(Error::State(
                "Address must be BLS address to load bls public key".to_owned(),
            )),
        }
    }

    /// Return the heaviest tipset's balance from self.db for a given address
    pub async fn get_heaviest_balance(&self, addr: &Address) -> Result<BigInt, Error> {
        let ts = self
            .cs
            .heaviest_tipset()
            .await
            .ok_or_else(|| Error::Other("could not get bs heaviest ts".to_owned()))?;
        let cid = ts.parent_state();
        self.get_balance(addr, cid)
    }

    /// Return the balance of a given address and state_cid
    pub fn get_balance(&self, addr: &Address, cid: &Cid) -> Result<BigInt, Error> {
        let act = self.get_actor(addr, cid)?;
        let actor = act.ok_or_else(|| "could not find actor".to_owned())?;
        Ok(actor.balance)
    }

    /// Looks up ID [Address] from the state at the given [Tipset].
    pub fn lookup_id(&self, addr: &Address, ts: &Tipset) -> Result<Option<Address>, Error> {
        let state_tree = StateTree::new_from_root(self.blockstore(), ts.parent_state())
            .map_err(|e| e.to_string())?;
        Ok(state_tree.lookup_id(addr)?)
    }

    /// Retrieves market balance in escrow and locked tables.
    pub fn market_balance(&self, addr: &Address, ts: &Tipset) -> Result<MarketBalance, Error> {
        let actor = self
            .get_actor(actor::market::ADDRESS, ts.parent_state())?
            .ok_or_else(|| Error::State("Power actor address could not be resolved".to_string()))?;

        let market_state = market::State::load(self.blockstore(), &actor)?;

        let new_addr = self
            .lookup_id(addr, ts)?
            .ok_or_else(|| Error::State(format!("Failed to resolve address {}", addr)))?;

        let out = MarketBalance {
            escrow: {
                market_state
                    .escrow_table(self.blockstore())?
                    .get(&new_addr)?
            },
            locked: {
                market_state
                    .locked_table(self.blockstore())?
                    .get(&new_addr)?
            },
        };

        Ok(out)
    }

    /// Similar to `resolve_to_key_addr` in the vm crate but does not allow `Actor` type of addresses.
    /// Uses `ts` to generate the VM state.
    pub async fn resolve_to_key_addr<V>(
        self: &Arc<Self>,
        addr: &Address,
        ts: &Arc<Tipset>,
    ) -> Result<Address, Box<dyn StdError>>
    where
        V: ProofVerifier,
    {
        match addr.protocol() {
            Protocol::BLS | Protocol::Secp256k1 => return Ok(*addr),
            Protocol::Actor => {
                return Err(
                    Error::Other("cannot resolve actor address to key address".to_string()).into(),
                )
            }
            _ => {}
        };
        let (st, _) = self.tipset_state::<V>(ts).await?;
        let state = StateTree::new_from_root(self.blockstore(), &st)?;

        Ok(interpreter::resolve_to_key_addr(
            &state,
            self.blockstore(),
            &addr,
        )?)
    }

    /// Checks power actor state for if miner meets consensus minimum requirements.
    pub fn miner_has_min_power(
        &self,
        addr: &Address,
        ts: &Tipset,
    ) -> Result<bool, Box<dyn StdError>> {
        let actor = self
            .get_actor(actor::power::ADDRESS, ts.parent_state())?
            .ok_or_else(|| Error::State("Power actor address could not be resolved".to_string()))?;
        let ps = power::State::load(self.blockstore(), &actor)?;

        ps.miner_nominal_power_meets_consensus_minimum(self.blockstore(), addr)
    }

    pub async fn validate_chain<V: ProofVerifier>(
        self: &Arc<Self>,
        mut ts: Arc<Tipset>,
        height: i64,
    ) -> Result<(), Box<dyn StdError>> {
        let mut ts_chain = Vec::<Arc<Tipset>>::new();
        while ts.epoch() != height {
            let next = self.cs.tipset_from_keys(ts.parents()).await?;
            ts_chain.push(std::mem::replace(&mut ts, next));
        }
        ts_chain.push(ts);

        let mut last_state = *ts_chain.last().unwrap().parent_state();
        let mut last_receipt = *ts_chain.last().unwrap().blocks()[0].message_receipts();
        for ts in ts_chain.iter().rev() {
            if ts.parent_state() != &last_state {
                #[cfg(feature = "statediff")]
                statediff::print_state_diff(
                    self.blockstore(),
                    &last_state,
                    &ts.parent_state(),
                    Some(1),
                )
                .unwrap();

                return Err(format!(
                    "Tipset chain has state mismatch at height: {}, {} != {}, \
                        receipts mismatched: {}",
                    ts.epoch(),
                    ts.parent_state(),
                    last_state,
                    ts.blocks()[0].message_receipts() != &last_receipt
                )
                .into());
            }
            if ts.blocks()[0].message_receipts() != &last_receipt {
                return Err(format!(
                    "Tipset message receipts has a mismatch at height: {}",
                    ts.epoch(),
                )
                .into());
            }
            info!(
                "Computing state (height: {}, ts={:?})",
                ts.epoch(),
                ts.cids()
            );
            let (st, msg_root) = self.tipset_state::<V>(&ts).await?;
            last_state = st;
            last_receipt = msg_root;
        }
        Ok(())
    }

    /// Retrieves total circulating supply on the network.
    pub fn get_circulating_supply(
        self: &Arc<Self>,
        height: ChainEpoch,
        state_tree: &StateTree<DB>,
    ) -> Result<TokenAmount, Box<dyn StdError>> {
        self.genesis_info.get_supply(height, state_tree)
    }

    /// Return the state of Market Actor.
    pub fn get_market_state(&self, ts: &Tipset) -> Result<market::State, Error> {
        let actor = self
            .get_actor(actor::market::ADDRESS, ts.parent_state())?
            .ok_or_else(|| {
                Error::State("Market actor address could not be resolved".to_string())
            })?;

        let market_state = market::State::load(self.blockstore(), &actor)?;
        Ok(market_state)
    }
}

/// Base miner info needed for the RPC API.
// * There is not a great reason this is a separate type from the one on the RPC.
// * This should probably be removed in the future, but is a convenience to keep for now.
pub struct MiningBaseInfo {
    pub miner_power: Option<TokenAmount>,
    pub network_power: Option<TokenAmount>,
    pub sectors: Vec<SectorInfo>,
    pub worker_key: Address,
    pub sector_size: SectorSize,
    pub prev_beacon_entry: BeaconEntry,
    pub beacon_entries: Vec<BeaconEntry>,
    pub eligible_for_mining: bool,
}

struct SMLookbackWrapper<'sm, 'ts, DB, BS, V> {
    sm: &'sm Arc<StateManager<DB>>,
    store: &'sm BS,
    tipset: &'ts Arc<Tipset>,
    verifier: PhantomData<V>,
}

impl<'sm, 'ts, DB, BS, V> LookbackStateGetter<'sm, BS> for SMLookbackWrapper<'sm, 'ts, DB, BS, V>
where
    // Yes, both are needed, because the VM should only use the buffered store
    DB: BlockStore + Send + Sync + 'static,
    BS: BlockStore + Send + Sync,
    V: ProofVerifier,
{
    fn state_lookback(&self, round: ChainEpoch) -> Result<StateTree<'sm, BS>, Box<dyn StdError>> {
        let (_, st) = task::block_on(
            self.sm
                .get_lookback_tipset_for_round::<V>(self.tipset.clone(), round),
        )?;

        StateTree::new_from_root(self.store, &st)
    }
}
