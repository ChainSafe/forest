// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod chain_rand;
mod errors;
mod utils;
mod vm_circ_supply;

pub use self::errors::*;
use actor_interface::*;
use anyhow::Context;
use async_log::span;
use async_std::{sync::RwLock, task};
use beacon::{Beacon, BeaconEntry, BeaconSchedule, DrandBeacon, IGNORE_DRAND_VAR};
use chain::{ChainStore, HeadChange};
use chain_rand::ChainRand;
use cid::Cid;
use fil_actors_runtime::runtime::{DomainSeparationTag, Policy};
use fil_types::{verifier::ProofVerifier, SectorInfo, SectorSize};
use forest_blocks::{BlockHeader, Tipset, TipsetKeys};
use forest_message::{message_receipt, ChainMessage, Message as MessageTrait, MessageReceipt};
use forest_vm::TokenAmount;
use futures::{channel::oneshot, select, FutureExt};
use fvm::executor::ApplyRet;
use fvm::externs::Rand;
use fvm::machine::NetworkConfig;
use fvm::state_tree::{ActorState, StateTree};
use fvm_ipld_encoding::Cbor;
use fvm_shared::address::{Address, Payload, Protocol, BLS_PUB_LEN};
use fvm_shared::bigint::{bigint_ser, BigInt};
use fvm_shared::clock::ChainEpoch;
use fvm_shared::message::Message;
use fvm_shared::randomness::Randomness;
use fvm_shared::version::NetworkVersion;
use interpreter::{
    resolve_to_key_addr, BlockMessages, CircSupplyCalc, Heights, LookbackStateGetter, RewardCalc,
    VM,
};
use ipld_blockstore::{BlockStore, BlockStoreExt};
use legacy_ipld_amt::Amt;
use log::{debug, info, trace, warn};
use networks::{ChainConfig, Height};
use num_traits::identities::Zero;
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::broadcast::{error::RecvError, Receiver as Subscriber, Sender as Publisher};
use vm_circ_supply::GenesisInfo;

/// Intermediary for retrieving state objects and updating actor states.
type CidPair = (Cid, Cid);

/// Type to represent invocation of state call results.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct InvocResult {
    #[serde(with = "forest_message::message::json")]
    pub msg: Message,
    #[serde(with = "message_receipt::json::opt")]
    pub msg_rct: Option<MessageReceipt>,
    pub error: Option<String>,
}

/// An alias Result that represents an `InvocResult` and an Error.
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
/// This encapsulates the [`ChainStore`] functionality, which only handles chain data, to
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
    beacon: Arc<beacon::BeaconSchedule<DrandBeacon>>,
    chain_config: Arc<ChainConfig>,
    engine: fvm::machine::MultiEngine,
    reward_calc: Arc<dyn RewardCalc>,
}

impl<DB> StateManager<DB>
where
    DB: BlockStore + Send + Sync + 'static,
{
    pub async fn new(
        cs: Arc<ChainStore<DB>>,
        chain_config: Arc<ChainConfig>,
        reward_calc: Arc<dyn RewardCalc>,
    ) -> Result<Self, anyhow::Error> {
        let genesis = cs.genesis()?.context("genesis header missing")?;
        let beacon = Arc::new(
            chain_config
                .get_beacon_schedule(genesis.timestamp())
                .await?,
        );

        Ok(Self {
            cs,
            cache: RwLock::new(HashMap::new()),
            publisher: None,
            genesis_info: GenesisInfo::from_chain_config(&chain_config),
            beacon,
            chain_config,
            engine: fvm::machine::MultiEngine::new(),
            reward_calc,
        })
    }

    /// Creates a constructor that passes in a `HeadChange` publisher.
    pub async fn new_with_publisher(
        cs: Arc<ChainStore<DB>>,
        chain_subs: Publisher<HeadChange>,
        config: ChainConfig,
        reward_calc: Arc<dyn RewardCalc>,
    ) -> Result<Self, anyhow::Error> {
        let genesis = cs.genesis()?.context("genesis header missing")?;
        let chain_config = Arc::new(config);
        let beacon = Arc::new(
            chain_config
                .get_beacon_schedule(genesis.timestamp())
                .await?,
        );

        Ok(Self {
            cs,
            cache: RwLock::new(HashMap::new()),
            publisher: Some(chain_subs),
            genesis_info: GenesisInfo::from_chain_config(&chain_config),
            beacon,
            chain_config,
            engine: fvm::machine::MultiEngine::new(),
            reward_calc,
        })
    }

    pub fn beacon_schedule(&self) -> Arc<BeaconSchedule<DrandBeacon>> {
        self.beacon.clone()
    }

    /// Returns network version for the given epoch.
    pub fn get_network_version(&self, epoch: ChainEpoch) -> NetworkVersion {
        self.chain_config.network_version(epoch)
    }

    pub fn chain_config(&self) -> &Arc<ChainConfig> {
        &self.chain_config
    }

    /// Gets actor from given [`Cid`], if it exists.
    pub fn get_actor(&self, addr: &Address, state_cid: Cid) -> Result<Option<ActorState>, Error> {
        let state = StateTree::new_from_root(self.blockstore_cloned(), &state_cid)?;
        Ok(state.get_actor(addr)?)
    }

    /// Returns the cloned [`Arc`] of the state manager's [`BlockStore`].
    pub fn blockstore_cloned(&self) -> DB {
        self.cs.blockstore_cloned()
    }

    /// Returns a reference to the state manager's [`BlockStore`].
    pub fn blockstore(&self) -> &DB {
        self.cs.blockstore()
    }

    /// Returns reference to the state manager's [`ChainStore`].
    pub fn chain_store(&self) -> &Arc<ChainStore<DB>> {
        &self.cs
    }

    /// Gets 32 bytes of randomness for `ChainRand` parameterized by the `DomainSeparationTag`, `ChainEpoch`,
    /// Entropy from the latest beacon entry.
    pub async fn get_beacon_randomness(
        &self,
        blocks: &TipsetKeys,
        pers: i64,
        round: ChainEpoch,
        entropy: &[u8],
    ) -> anyhow::Result<[u8; 32]> {
        let chain_rand = self.chain_rand(blocks.to_owned());
        match self.get_network_version(round) {
            NetworkVersion::V16 | NetworkVersion::V15 | NetworkVersion::V14 => {
                chain_rand
                    .get_beacon_randomness_v3(blocks, pers, round, entropy)
                    .await
            }
            NetworkVersion::V13 => {
                chain_rand
                    .get_beacon_randomness_v2(blocks, pers, round, entropy)
                    .await
            }
            NetworkVersion::V0
            | NetworkVersion::V1
            | NetworkVersion::V2
            | NetworkVersion::V3
            | NetworkVersion::V4
            | NetworkVersion::V5
            | NetworkVersion::V6
            | NetworkVersion::V7
            | NetworkVersion::V8
            | NetworkVersion::V9
            | NetworkVersion::V10
            | NetworkVersion::V11
            | NetworkVersion::V12 => {
                chain_rand
                    .get_beacon_randomness_v1(blocks, pers, round, entropy)
                    .await
            }
            _ => panic!("Unsupported network version"),
        }
    }

    /// Gets 32 bytes of randomness for `ChainRand` parameterized by the `DomainSeparationTag`, `ChainEpoch`,
    /// Entropy from the ticket chain.
    pub async fn get_chain_randomness(
        &self,
        blocks: &TipsetKeys,
        pers: i64,
        round: ChainEpoch,
        entropy: &[u8],
        lookback: bool,
    ) -> anyhow::Result<[u8; 32]> {
        let chain_rand = self.chain_rand(blocks.to_owned());
        chain_rand
            .get_chain_randomness(blocks, pers, round, entropy, lookback)
            .await
    }

    // This function used to do this: Returns the network name from the init actor state.
    /// Returns the internal, protocol-level network name.
    pub fn get_network_name(&self, _st: &Cid) -> Result<String, Error> {
        if self.chain_config.name == "calibnet" {
            return Ok("calibrationnet".to_owned());
        }
        if self.chain_config.name == "mainnet" {
            return Ok("testnetnet".to_owned());
        }
        if self.chain_config.name == "devnet" {
            return Ok("devnet".to_owned());
        }
        Err(Error::Other("Cannot guess network name".to_owned()))
        // let init_act = self
        //     .get_actor(actor::init::ADDRESS, *st)?
        //     .ok_or_else(|| Error::State("Init actor address could not be resolved".to_string()))?;
        // let state = init::State::load(self.blockstore(), &init_act)?;
        // Ok(state.into_network_name())
    }

    /// Returns true if miner has been slashed or is considered invalid.
    pub fn is_miner_slashed(&self, addr: &Address, state_cid: &Cid) -> anyhow::Result<bool, Error> {
        let actor = self
            .get_actor(&actor_interface::power::ADDRESS, *state_cid)?
            .ok_or_else(|| Error::State("Power actor address could not be resolved".to_string()))?;

        let spas = power::State::load(self.blockstore(), &actor)?;

        Ok(spas.miner_power(self.blockstore(), addr)?.is_none())
    }

    /// Returns raw work address of a miner given the state root.
    pub fn get_miner_work_addr(
        &self,
        state_cid: Cid,
        addr: &Address,
    ) -> anyhow::Result<Address, Error> {
        let state = StateTree::new_from_root(self.blockstore(), &state_cid)?;

        let act = state
            .get_actor(addr)?
            .ok_or_else(|| Error::State("Miner actor not found".to_string()))?;

        let ms = miner::State::load(self.blockstore(), &act)?;

        let info = ms.info(self.blockstore()).map_err(|e| e.to_string())?;

        let addr = resolve_to_key_addr(&state, self.blockstore(), &info.worker())?;
        Ok(addr)
    }

    /// Returns specified actor's claimed power and total network power as a tuple.
    pub fn get_power(
        &self,
        state_cid: &Cid,
        addr: Option<&Address>,
    ) -> anyhow::Result<Option<(power::Claim, power::Claim)>, Error> {
        let actor = self
            .get_actor(&actor_interface::power::ADDRESS, *state_cid)?
            .ok_or_else(|| Error::State("Power actor address could not be resolved".to_string()))?;

        let spas = power::State::load(self.blockstore(), &actor)?;

        let t_pow = spas.total_power();

        if let Some(maddr) = addr {
            let m_pow = spas
                .miner_power(self.blockstore(), maddr)?
                .ok_or_else(|| Error::State(format!("Miner for address {} not found", maddr)))?;

            let min_pow = spas.miner_nominal_power_meets_consensus_minimum(
                &self.chain_config.policy,
                self.blockstore(),
                maddr,
            )?;
            if min_pow {
                return Ok(Some((m_pow, t_pow)));
            }
        }

        Ok(None)
    }

    /// Subscribes to the [`HeadChange`]s observed by the state manager.
    pub fn get_subscriber(&self) -> Option<Subscriber<HeadChange>> {
        self.publisher.as_ref().map(|p| p.subscribe())
    }

    /// Performs the state transition for the tipset and applies all unique messages in all blocks.
    /// This function returns the state root and receipt root of the transition.
    #[allow(clippy::too_many_arguments)]
    pub fn apply_blocks<R, CB>(
        self: &Arc<Self>,
        parent_epoch: ChainEpoch,
        p_state: &Cid,
        messages: &[BlockMessages],
        epoch: ChainEpoch,
        rand: &R,
        base_fee: BigInt,
        mut callback: Option<CB>,
        tipset: &Arc<Tipset>,
    ) -> Result<CidPair, anyhow::Error>
    where
        R: Rand + Clone + 'static,
        CB: FnMut(&Cid, &ChainMessage, &ApplyRet) -> Result<(), anyhow::Error>,
    {
        let db = self.blockstore_cloned();
        let lb_wrapper = SMLookbackWrapper {
            sm: Arc::clone(self),
            tipset: Arc::clone(tipset),
        };

        let turbo_height = self.chain_config.epoch(Height::Turbo);
        let rand_clone = rand.clone();
        let create_vm = |state_root, epoch| {
            let heights = Heights::new(&self.chain_config);
            let network_version = self.get_network_version(epoch);
            VM::<_>::new(
                state_root,
                db.clone(),
                epoch,
                &rand_clone,
                base_fee.clone(),
                network_version,
                self.genesis_info.clone(),
                self.reward_calc.clone(),
                None,
                &lb_wrapper,
                self.engine
                    .get(&NetworkConfig::new(network_version))
                    .unwrap(),
                heights,
                self.chain_config.policy.chain_finality,
            )
        };

        let mut parent_state = *p_state;

        for epoch_i in parent_epoch..epoch {
            if epoch_i > parent_epoch {
                let mut vm = create_vm(parent_state, epoch_i)?;
                // run cron for null rounds if any
                if let Err(e) = vm.run_cron(epoch_i, callback.as_mut()) {
                    log::error!("Beginning of epoch cron failed to run: {}", e);
                }

                parent_state = vm.flush()?;
            }

            if epoch_i == turbo_height {
                todo!("cannot migrate state when using FVM - see https://github.com/ChainSafe/forest/issues/1454 for updates");
            }
        }

        let mut vm = create_vm(parent_state, epoch)?;

        // Apply tipset messages
        let receipts = vm.apply_block_messages(messages, epoch, callback)?;

        // Construct receipt root from receipts
        let receipt_root = Amt::new_from_iter(self.blockstore(), receipts)?;

        // Flush changes to blockstore
        let state_root = vm.flush()?;

        // FIXME: Buffering disabled while debugging. Investigate if the buffer improves performance.
        //        See issue: https://github.com/ChainSafe/forest/issues/1451
        // Persist changes connected to root
        // buf_store
        //     .flush(&state_root)
        //     .expect("buffered blockstore flush failed");

        Ok((state_root, receipt_root))
    }

    /// Returns the pair of (parent state root, message receipt root). This will either be cached
    /// or will be calculated and fill the cache. Tipset state for a given tipset is guaranteed
    /// not to be computed twice.
    pub async fn tipset_state(
        self: &Arc<Self>,
        tipset: &Arc<Tipset>,
    ) -> Result<CidPair, anyhow::Error> {
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
                let no_func =
                    None::<fn(&Cid, &ChainMessage, &ApplyRet) -> Result<(), anyhow::Error>>;
                let ts_state = self.compute_tipset_state(tipset, no_func).await?;
                debug!("completed tipset state calculation {:?}", tipset.cids());
                ts_state
            };

            // Fill entry with calculated cid pair
            *entry_lock = Some(cid_pair);
            Ok(cid_pair)
        })
    }

    fn call_raw(
        self: &Arc<Self>,
        msg: &mut Message,
        rand: &ChainRand<DB>,
        tipset: &Arc<Tipset>,
    ) -> StateCallResult {
        span!("state_call_raw", {
            let bstate = tipset.parent_state();
            let bheight = tipset.epoch();

            let lb_wrapper = SMLookbackWrapper {
                sm: Arc::clone(self),
                tipset: Arc::clone(tipset),
            };

            let store_arc = self.blockstore_cloned();

            let heights = Heights::new(&self.chain_config);
            let network_version = self.get_network_version(bheight);
            let mut vm = VM::<_>::new(
                *bstate,
                store_arc,
                bheight,
                rand,
                0.into(),
                network_version,
                self.genesis_info.clone(),
                self.reward_calc.clone(),
                None,
                &lb_wrapper,
                self.engine
                    .get(&NetworkConfig::new(network_version))
                    .unwrap(),
                heights,
                self.chain_config.policy.chain_finality,
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
            if let Some(err) = &apply_ret.failure_info {
                warn!("chain call failed: {:?}", err);
            }

            Ok(InvocResult {
                msg: msg.clone(),
                msg_rct: Some(apply_ret.msg_receipt.clone()),
                error: apply_ret.failure_info.map(|e| e.to_string()),
            })
        })
    }

    /// runs the given message and returns its result without any persisted changes.
    pub async fn call(
        self: &Arc<Self>,
        message: &mut Message,
        tipset: Option<Arc<Tipset>>,
    ) -> StateCallResult {
        let ts = if let Some(t_set) = tipset {
            t_set
        } else {
            self.cs
                .heaviest_tipset()
                .await
                .ok_or_else(|| Error::Other("No heaviest tipset".to_string()))?
        };
        let chain_rand = self.chain_rand(ts.key().to_owned());
        self.call_raw(message, &chain_rand, &ts)
    }

    /// Computes message on the given [Tipset] state, after applying other messages and returns
    /// the values computed in the VM.
    pub async fn call_with_gas(
        self: &Arc<Self>,
        message: &mut ChainMessage,
        prior_messages: &[ChainMessage],
        tipset: Option<Arc<Tipset>>,
    ) -> StateCallResult {
        let ts = if let Some(t_set) = tipset {
            t_set
        } else {
            self.cs
                .heaviest_tipset()
                .await
                .ok_or_else(|| Error::Other("No heaviest tipset".to_string()))?
        };
        let (st, _) = self
            .tipset_state(&ts)
            .await
            .map_err(|_| Error::Other("Could not load tipset state".to_string()))?;
        let chain_rand = self.chain_rand(ts.key().to_owned());

        // TODO investigate: this doesn't use a buffered store in any way, and can lead to
        // state bloat potentially?
        let lb_wrapper = SMLookbackWrapper {
            sm: Arc::clone(self),
            tipset: Arc::clone(&ts),
        };
        let store_arc = self.blockstore_cloned();
        let heights = Heights::new(&self.chain_config);
        // Since we're simulating a future message, pretend we're applying it in the "next" tipset
        let network_version = self.get_network_version(ts.epoch() + 1);
        let mut vm = VM::<_>::new(
            st,
            store_arc,
            ts.epoch() + 1,
            &chain_rand,
            ts.blocks()[0].parent_base_fee().clone(),
            network_version,
            self.genesis_info.clone(),
            self.reward_calc.clone(),
            None,
            &lb_wrapper,
            self.engine
                .get(&NetworkConfig::new(network_version))
                .unwrap(),
            heights,
            self.chain_config.policy.chain_finality,
        )?;

        for msg in prior_messages {
            vm.apply_message(msg)?;
        }
        let from_actor = vm
            .get_actor(message.from())
            .map_err(|e| Error::Other(format!("Could not get actor from state: {}", e)))?
            .ok_or_else(|| Error::Other("cant find actor in state tree".to_string()))?;
        message.set_sequence(from_actor.sequence);

        let ret = vm.apply_message(message)?;

        Ok(InvocResult {
            msg: message.message().clone(),
            msg_rct: Some(ret.msg_receipt.clone()),
            error: ret.failure_info.map(|e| e.to_string()),
        })
    }

    /// Replays the given message and returns the result of executing the indicated message,
    /// assuming it was executed in the indicated tipset.
    pub async fn replay(
        self: &Arc<Self>,
        ts: &Arc<Tipset>,
        mcid: Cid,
    ) -> Result<(Message, ApplyRet), Error> {
        // This isn't ideal to have, since the execution is syncronous, but this needs to be the
        // case because the state transition has to be in blocking thread to avoid starving executor
        let outm: OnceCell<Message> = Default::default();
        let outr: OnceCell<ApplyRet> = Default::default();
        let m_clone = outm.clone();
        let r_clone = outr.clone();
        let callback = move |cid: &Cid, unsigned: &ChainMessage, apply_ret: &ApplyRet| {
            if *cid == mcid {
                let _ = m_clone.set(unsigned.message().clone());
                let _ = r_clone.set(apply_ret.clone());
                anyhow::bail!("halt");
            }

            Ok(())
        };
        let result = self.compute_tipset_state(ts, Some(callback)).await;

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

    /// Gets look-back tipset for block validations.
    pub async fn get_lookback_tipset_for_round(
        self: &Arc<Self>,
        tipset: Arc<Tipset>,
        round: ChainEpoch,
    ) -> Result<(Arc<Tipset>, Cid), Error> {
        let mut lbr: ChainEpoch = ChainEpoch::from(0);
        let version = self.get_network_version(round);
        let lb = if version <= NetworkVersion::V3 {
            ChainEpoch::from(10)
        } else {
            self.chain_config.policy.chain_finality
        };

        if round > lb {
            lbr = round - lb
        }

        // More null blocks than lookback
        if lbr >= tipset.epoch() {
            let (st, _) = self
                .tipset_state(&tipset)
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
            .get_actor(
                &actor_interface::power::ADDRESS,
                *base_tipset.parent_state(),
            )?
            .ok_or_else(|| Error::State("Power actor address could not be resolved".to_string()))?;

        let power_state = power::State::load(self.blockstore(), &actor)?;

        let actor = self
            .get_actor(address, *base_tipset.parent_state())?
            .ok_or_else(|| Error::State("Miner actor address could not be resolved".to_string()))?;

        let miner_state = miner::State::load(self.blockstore(), &actor)?;

        // Non-empty power claim.
        let claim = power_state
            .miner_power(self.blockstore(), address)?
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

    /// Gets a miner's base info from state, based on the address provided.
    pub async fn miner_get_base_info<V: ProofVerifier, B: Beacon>(
        self: &Arc<Self>,
        beacon: &BeaconSchedule<B>,
        key: &TipsetKeys,
        round: ChainEpoch,
        address: Address,
    ) -> Result<Option<MiningBaseInfo>, anyhow::Error> {
        let tipset = self.cs.tipset_from_keys(key).await?;
        let prev = match self.cs.latest_beacon_entry(&tipset).await {
            Ok(prev) => prev,
            Err(err) => {
                if std::env::var(IGNORE_DRAND_VAR)
                    .map(|e| e != "1")
                    .unwrap_or(true)
                {
                    anyhow::bail!("failed to get latest beacon entry: {:?}", err);
                }
                beacon::BeaconEntry::default()
            }
        };
        let entries = beacon
            .beacon_entries_for_block(
                self.get_network_version(round),
                round,
                tipset.epoch(),
                &prev,
            )
            .await?;
        let rbase = entries.iter().last().unwrap_or(&prev);
        let (lbts, lbst) = self
            .get_lookback_tipset_for_round(tipset.clone(), round)
            .await?;

        let actor = self
            .get_actor(&address, lbst)?
            .ok_or_else(|| Error::State("Power actor address could not be resolved".to_string()))?;
        let miner_state = miner::State::load(self.blockstore(), &actor)?;

        let buf = address.marshal_cbor()?;
        let prand = chain_rand::draw_randomness(
            rbase.data(),
            DomainSeparationTag::WinningPoStChallengeSeed as i64,
            round,
            &buf,
        )?;

        let nv = self.get_network_version(tipset.epoch());
        let sectors = self.get_sectors_for_winning_post::<V>(
            &lbst,
            nv,
            &address,
            Randomness(prand.to_vec()),
        )?;

        if sectors.is_empty() {
            return Ok(None);
        }

        let (mpow, tpow) = self
            .get_power(&lbst, Some(&address))?
            .ok_or_else(|| Error::State(format!("failed to load power for address {}", address)))?;

        let info = miner_state.info(self.blockstore())?;

        let (st, _) = self.tipset_state(&lbts).await?;
        let state = StateTree::new_from_root(self.blockstore(), &st)?;

        let worker_key = resolve_to_key_addr(&state, self.blockstore(), &info.worker())?;

        let eligible = self.eligible_to_mine(&address, tipset.as_ref(), &lbts)?;

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
    pub async fn compute_tipset_state<CB: 'static>(
        self: &Arc<Self>,
        tipset: &Arc<Tipset>,
        callback: Option<CB>,
    ) -> Result<CidPair, Error>
    where
        CB: FnMut(&Cid, &ChainMessage, &ApplyRet) -> Result<(), anyhow::Error> + Send,
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
                    .get_obj(parent_cid)?
                    .ok_or_else(|| format!("Could not find parent block with cid {parent_cid}"))?;
                parent.epoch()
            } else {
                Default::default()
            };

            let tipset_keys =
                TipsetKeys::new(block_headers.iter().map(|s| s.cid()).cloned().collect());
            let chain_rand = self.chain_rand(tipset_keys);
            let base_fee = first_block.parent_base_fee().clone();

            let blocks = self
                .chain_store()
                .block_msgs_for_tipset(tipset)
                .map_err(|e| Error::Other(e.to_string()))?;

            let sm = Arc::clone(self);
            let sr = *first_block.state_root();
            let epoch = first_block.epoch();
            let ts_cloned = Arc::clone(tipset);
            task::spawn_blocking(move || {
                Ok(sm.apply_blocks(
                    parent_epoch,
                    &sr,
                    &blocks,
                    epoch,
                    &chain_rand,
                    base_fee,
                    callback,
                    &ts_cloned,
                )?)
            })
            .await
        })
    }

    /// Check if tipset had executed the message, by loading the receipt based on the index of
    /// the message in the block.
    async fn tipset_executed_message(
        &self,
        tipset: &Tipset,
        msg_cid: Cid,
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
                        s == msg_cid
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
                *message_cid,
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
    /// Returns a message receipt from a given tipset and message CID.
    pub async fn get_receipt(&self, tipset: &Tipset, msg: Cid) -> Result<MessageReceipt, Error> {
        let m = chain::get_chain_message(self.blockstore(), &msg)
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

    /// `WaitForMessage` blocks until a message appears on chain. It looks backwards in the
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
            .tipset_executed_message(&current_tipset, msg_cid, message_var)
            .await?;
        if let Some(r) = maybe_message_reciept {
            return Ok((Some(current_tipset.clone()), Some(r)));
        }

        let mut candidate_tipset: Option<Arc<Tipset>> = None;
        let mut candidate_receipt: Option<MessageReceipt> = None;

        let sm_cloned = Arc::clone(self);
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
        let sm_cloned = Arc::clone(self);

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
                                .tipset_executed_message(&tipset, msg_cid, message_var)
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

    /// Returns a BLS public key from provided address
    pub fn get_bls_public_key(
        db: &DB,
        addr: &Address,
        state_cid: Cid,
    ) -> Result<[u8; BLS_PUB_LEN], Error> {
        let state = StateTree::new_from_root(db, &state_cid)?;
        let kaddr = resolve_to_key_addr(&state, db, addr)
            .map_err(|e| format!("Failed to resolve key address, error: {}", e))?;

        match kaddr.into_payload() {
            Payload::BLS(key) => Ok(key),
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
        self.get_balance(addr, *cid)
    }

    /// Return the balance of a given address and `state_cid`
    pub fn get_balance(&self, addr: &Address, cid: Cid) -> Result<BigInt, Error> {
        let act = self.get_actor(addr, cid)?;
        let actor = act.ok_or_else(|| "could not find actor".to_owned())?;
        Ok(actor.balance)
    }

    /// Looks up ID [Address] from the state at the given [Tipset].
    pub fn lookup_id(&self, addr: &Address, ts: &Tipset) -> Result<Option<Address>, Error> {
        let state_tree = StateTree::new_from_root(self.blockstore(), ts.parent_state())
            .map_err(|e| e.to_string())?;
        Ok(state_tree.lookup_id(addr)?.map(Address::new_id))
    }

    /// Retrieves market balance in escrow and locked tables.
    pub fn market_balance(
        &self,
        addr: &Address,
        ts: &Tipset,
    ) -> anyhow::Result<MarketBalance, Error> {
        let actor = self
            .get_actor(&actor_interface::market::ADDRESS, *ts.parent_state())?
            .ok_or_else(|| {
                Error::State("Market actor address could not be resolved".to_string())
            })?;

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

    /// Similar to `resolve_to_key_addr` in the `forest_vm` crate but does not allow `Actor` type of addresses.
    /// Uses `ts` to generate the VM state.
    pub async fn resolve_to_key_addr(
        self: &Arc<Self>,
        addr: &Address,
        ts: &Arc<Tipset>,
    ) -> Result<Address, anyhow::Error> {
        match addr.protocol() {
            Protocol::BLS | Protocol::Secp256k1 => return Ok(*addr),
            Protocol::Actor => {
                return Err(
                    Error::Other("cannot resolve actor address to key address".to_string()).into(),
                )
            }
            _ => {}
        };
        let (st, _) = self.tipset_state(ts).await?;
        let state = StateTree::new_from_root(self.blockstore(), &st)?;

        resolve_to_key_addr(&state, self.blockstore(), addr)
    }

    /// Checks power actor state for if miner meets consensus minimum requirements.
    pub fn miner_has_min_power(
        &self,
        policy: &Policy,
        addr: &Address,
        ts: &Tipset,
    ) -> anyhow::Result<bool> {
        let actor = self
            .get_actor(&actor_interface::power::ADDRESS, *ts.parent_state())?
            .ok_or_else(|| Error::State("Power actor address could not be resolved".to_string()))?;
        let ps = power::State::load(self.blockstore(), &actor)?;

        ps.miner_nominal_power_meets_consensus_minimum(policy, self.blockstore(), addr)
    }

    pub async fn validate_chain<V: ProofVerifier>(
        self: &Arc<Self>,
        mut ts: Arc<Tipset>,
        height: i64,
    ) -> Result<(), anyhow::Error> {
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
                    ts.parent_state(),
                    Some(1),
                )
                .unwrap();

                anyhow::bail!(
                    "Tipset chain has state mismatch at height: {}, {} != {}, \
                        receipts mismatched: {}",
                    ts.epoch(),
                    ts.parent_state(),
                    last_state,
                    ts.blocks()[0].message_receipts() != &last_receipt
                );
            }
            if ts.blocks()[0].message_receipts() != &last_receipt {
                anyhow::bail!(
                    "Tipset message receipts has a mismatch at height: {}",
                    ts.epoch(),
                );
            }
            info!(
                "Computing state (height: {}, ts={:?})",
                ts.epoch(),
                ts.cids()
            );
            let (st, msg_root) = self.tipset_state(ts).await?;
            last_state = st;
            last_receipt = msg_root;
        }
        Ok(())
    }

    /// Retrieves total circulating supply on the network.
    pub fn get_circulating_supply(
        self: &Arc<Self>,
        height: ChainEpoch,
        state_tree: &StateTree<&DB>,
    ) -> Result<TokenAmount, anyhow::Error> {
        self.genesis_info.get_supply(height, state_tree)
    }

    /// Return the state of Market Actor.
    pub fn get_market_state(&self, ts: &Tipset) -> anyhow::Result<market::State> {
        let actor = self
            .get_actor(&actor_interface::market::ADDRESS, *ts.parent_state())?
            .ok_or_else(|| {
                Error::State("Market actor address could not be resolved".to_string())
            })?;

        let market_state = market::State::load(self.blockstore(), &actor)?;
        Ok(market_state)
    }

    fn chain_rand(&self, blocks: TipsetKeys) -> ChainRand<DB> {
        ChainRand::new(
            self.chain_config.clone(),
            blocks,
            self.cs.clone(),
            self.beacon.clone(),
        )
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

struct SMLookbackWrapper<DB> {
    sm: Arc<StateManager<DB>>,
    tipset: Arc<Tipset>,
}

impl<DB> LookbackStateGetter for SMLookbackWrapper<DB>
where
    // Yes, both are needed, because the VM should only use the buffered store
    DB: BlockStore + Send + Sync + 'static,
{
    fn chain_epoch_root(&self) -> Box<dyn Fn(ChainEpoch) -> Cid> {
        let sm = Arc::clone(&self.sm);
        let tipset = Arc::clone(&self.tipset);
        Box::new(move |round| {
            let (_, st) = task::block_on(sm.get_lookback_tipset_for_round(tipset.clone(), round))
                .unwrap_or_else(|err| {
                    panic!("Internal Error. Failed to find root CID for epoch {round}: {err}")
                });
            st
        })
    }
}
