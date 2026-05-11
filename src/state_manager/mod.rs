// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#[cfg(test)]
mod tests;

mod actor_queries;
mod address_resolution;
mod cache;
pub mod chain_rand;
pub mod circulating_supply;
mod errors;
mod message_search;
mod message_simulation;
pub mod utils;

pub use self::errors::*;
use self::utils::structured;

use crate::beacon::{BeaconEntry, BeaconSchedule};
use crate::blocks::{Tipset, TipsetKey};
use crate::chain::{
    ChainStore,
    index::{ChainIndex, ResolveNullTipset},
};
use crate::interpreter::{
    BlockMessages, CalledAt, ExecutionContext, IMPLICIT_MESSAGE_GAS_LIMIT, VM, resolve_to_key_addr,
};
use crate::interpreter::{MessageCallbackCtx, VMTrace};
use crate::lotus_json::{LotusJson, lotus_json_with_self};
use crate::message::{ChainMessage, MessageRead as _, MessageReadWrite as _, SignedMessage};
use crate::networks::ChainConfig;
use crate::rpc::state::{ApiInvocResult, InvocResult, MessageGasCost};
use crate::rpc::types::{MiningBaseInfo, SectorOnChainInfo};
use crate::shim::actors::init::{self, State};
use crate::shim::actors::miner::{MinerInfo, MinerPower, Partition};
use crate::shim::actors::verifreg::{Allocation, AllocationID, Claim};
use crate::shim::actors::*;
use crate::shim::address::AddressId;
use crate::shim::crypto::{Signature, SignatureType};
use crate::shim::{
    actors::{
        LoadActorStateFromBlockstore, miner::ext::MinerStateExt as _,
        verifreg::ext::VerifiedRegistryStateExt as _,
    },
    executor::{ApplyRet, Receipt, StampedEvent},
};
use crate::shim::{
    address::{Address, Payload, Protocol},
    clock::ChainEpoch,
    econ::TokenAmount,
    machine::{GLOBAL_MULTI_ENGINE, MultiEngine},
    message::Message,
    randomness::Randomness,
    runtime::Policy,
    state_tree::{ActorState, StateTree},
    version::NetworkVersion,
};
use crate::state_manager::cache::TipsetStateCache;
use crate::state_manager::chain_rand::draw_randomness;
use crate::state_migration::run_state_migrations;
use crate::utils::ShallowClone as _;
use crate::utils::cache::SizeTrackingLruCache;
use crate::utils::get_size::{GetSize, vec_heap_size_helper};
use ahash::{HashMap, HashMapExt};
use anyhow::{Context as _, bail, ensure};
use bls_signatures::{PublicKey as BlsPublicKey, Serialize as _};
use chain_rand::ChainRand;
use cid::Cid;
pub use circulating_supply::GenesisInfo;
use fil_actor_verifreg_state::v12::DataCap;
use fil_actor_verifreg_state::v13::ClaimID;
use fil_actors_shared::fvm_ipld_amt::{Amt, Amtv0};
use fil_actors_shared::fvm_ipld_bitfield::BitField;
use fil_actors_shared::v12::runtime::DomainSeparationTag;
use futures::{FutureExt, channel::oneshot, select};
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::to_vec;
use fvm_shared4::crypto::signature::SECP_SIG_LEN;
use itertools::Itertools as _;
use nonzero_ext::nonzero;
use num::BigInt;
use num_traits::identities::Zero;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::ops::RangeInclusive;
use std::time::Duration;
use std::{num::NonZeroUsize, sync::Arc};
use tokio::sync::{RwLock, broadcast::error::RecvError};
use tracing::{error, info, instrument, warn};

const DEFAULT_TIPSET_CACHE_SIZE: NonZeroUsize = nonzero!(1024usize);
const DEFAULT_ID_TO_DETERMINISTIC_ADDRESS_CACHE_SIZE: NonZeroUsize = nonzero!(1024usize);
pub const EVENTS_AMT_BITWIDTH: u32 = 5;
pub type IdToAddressCache = SizeTrackingLruCache<AddressId, Address>;

/// Result of executing an individual chain message in a tipset.
///
/// Includes the executed message itself, the execution receipt, and
/// optional events emitted by the actor during execution.
#[derive(Debug, Clone)]
pub struct ExecutedMessage {
    pub message: ChainMessage,
    pub receipt: Receipt,
    pub events: Option<Vec<StampedEvent>>,
}

impl GetSize for ExecutedMessage {
    fn get_heap_size(&self) -> usize {
        self.message.get_heap_size()
            + self.receipt.get_heap_size()
            + self
                .events
                .as_ref()
                .map(vec_heap_size_helper)
                .unwrap_or_default()
    }
}

/// Aggregated execution result for a tipset.
#[derive(Debug, Clone, GetSize)]
pub struct ExecutedTipset {
    /// Resulting state tree root after message execution
    #[get_size(ignore)]
    pub state_root: Cid,
    /// Resulting message receipts root after message execution
    #[get_size(ignore)]
    pub receipt_root: Cid,
    /// Per-message execution details.
    /// Wrapped in an `Arc` to reduce cloning cost, as this can be quite large.
    pub executed_messages: Arc<Vec<ExecutedMessage>>,
}

/// Basic execution result for a tipset.
#[derive(Debug, Clone, GetSize)]
pub struct TipsetState {
    /// Resulting state tree root after message execution
    #[get_size(ignore)]
    pub state_root: Cid,
    /// Resulting message receipts root after message execution
    #[allow(dead_code)]
    #[get_size(ignore)]
    pub receipt_root: Cid,
}

impl From<ExecutedTipset> for TipsetState {
    fn from(
        ExecutedTipset {
            state_root,
            receipt_root,
            ..
        }: ExecutedTipset,
    ) -> Self {
        Self {
            state_root,
            receipt_root,
        }
    }
}

impl From<&ExecutedTipset> for TipsetState {
    fn from(
        ExecutedTipset {
            state_root,
            receipt_root,
            ..
        }: &ExecutedTipset,
    ) -> Self {
        Self {
            state_root: *state_root,
            receipt_root: *receipt_root,
        }
    }
}

/// External format for returning market balance from state.
#[derive(
    Debug, Default, Serialize, Deserialize, Clone, PartialEq, Eq, PartialOrd, Ord, JsonSchema,
)]
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
    /// Chain store
    cs: Arc<ChainStore<DB>>,
    /// This is a cache which indexes tipsets to their calculated state output (state root, receipt root).
    cache: TipsetStateCache<ExecutedTipset>,
    id_to_deterministic_address_cache: IdToAddressCache,
    beacon: Arc<crate::beacon::BeaconSchedule>,
    engine: Arc<MultiEngine>,
}

#[allow(clippy::type_complexity)]
pub const NO_CALLBACK: Option<fn(MessageCallbackCtx<'_>) -> anyhow::Result<()>> = None;

impl<DB> StateManager<DB>
where
    DB: Blockstore,
{
    pub fn new(cs: Arc<ChainStore<DB>>) -> anyhow::Result<Self> {
        Self::new_with_engine(cs, GLOBAL_MULTI_ENGINE.clone())
    }

    pub fn new_with_engine(
        cs: Arc<ChainStore<DB>>,
        engine: Arc<MultiEngine>,
    ) -> anyhow::Result<Self> {
        let genesis = cs.genesis_block_header();
        let beacon = Arc::new(cs.chain_config().get_beacon_schedule(genesis.timestamp));

        Ok(Self {
            cs,
            cache: TipsetStateCache::new("executed_tipset"), // For StateOutput
            beacon,
            engine,
            id_to_deterministic_address_cache: SizeTrackingLruCache::new_with_metrics(
                "id_to_deterministic_address".into(),
                DEFAULT_ID_TO_DETERMINISTIC_ADDRESS_CACHE_SIZE,
            ),
        })
    }

    /// Returns the currently tracked heaviest tipset.
    pub fn heaviest_tipset(&self) -> Tipset {
        self.chain_store().heaviest_tipset()
    }

    /// Returns the currently tracked heaviest tipset and rewind to a most recent valid one if necessary.
    /// A valid head has
    ///     - state tree in the blockstore
    ///     - actor bundle version in the state tree that matches chain configuration
    pub fn maybe_rewind_heaviest_tipset(&self) -> anyhow::Result<()> {
        while self.maybe_rewind_heaviest_tipset_once()? {}
        Ok(())
    }

    fn maybe_rewind_heaviest_tipset_once(&self) -> anyhow::Result<bool> {
        let head = self.heaviest_tipset();
        if let Some(info) = self
            .chain_config()
            .network_height_with_actor_bundle(head.epoch())
        {
            let expected_height_info = info.info;
            let expected_bundle = info.manifest(self.blockstore())?;
            let expected_bundle_metadata = expected_bundle.metadata()?;
            let state = self.get_state_tree(head.parent_state())?;
            let bundle_metadata = state.get_actor_bundle_metadata()?;
            if expected_bundle_metadata != bundle_metadata {
                let current_epoch = head.epoch();
                let target_head = self.chain_index().load_required_tipset_by_height(
                    (expected_height_info.epoch - 1).max(0),
                    head,
                    ResolveNullTipset::TakeOlder,
                )?;
                let target_epoch = target_head.epoch();
                let bundle_version = &bundle_metadata.version;
                let expected_bundle_version = &expected_bundle_metadata.version;
                if target_epoch < current_epoch {
                    tracing::warn!(
                        "rewinding chain head from {current_epoch} to {target_epoch}, actor bundle: {bundle_version}, expected: {expected_bundle_version}"
                    );
                    if self.blockstore().has(target_head.parent_state())? {
                        self.chain_store().set_heaviest_tipset(target_head)?;
                        return Ok(true);
                    } else {
                        anyhow::bail!(
                            "failed to rewind, state tree @ {target_epoch} is missing from blockstore: {}",
                            target_head.parent_state()
                        );
                    }
                }
            }
        }
        Ok(false)
    }

    pub fn beacon_schedule(&self) -> &Arc<BeaconSchedule> {
        &self.beacon
    }

    /// Returns network version for the given epoch.
    pub fn get_network_version(&self, epoch: ChainEpoch) -> NetworkVersion {
        self.chain_config().network_version(epoch)
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
    pub fn blockstore(&self) -> &Arc<DB> {
        self.cs.blockstore()
    }

    pub fn blockstore_owned(&self) -> Arc<DB> {
        self.blockstore().clone()
    }

    /// Returns reference to the state manager's [`ChainStore`].
    pub fn chain_store(&self) -> &Arc<ChainStore<DB>> {
        &self.cs
    }

    /// Returns reference to the state manager's [`ChainIndex`].
    pub fn chain_index(&self) -> &ChainIndex<DB> {
        self.cs.chain_index()
    }

    /// Returns reference to the state manager's [`ChainConfig`].
    pub fn chain_config(&self) -> &Arc<ChainConfig> {
        self.cs.chain_config()
    }

    pub fn chain_rand(&self, tipset: Tipset) -> ChainRand<DB> {
        ChainRand::new(
            self.chain_config().shallow_clone(),
            tipset,
            self.chain_index().shallow_clone(),
            self.beacon.shallow_clone(),
        )
    }

    /// Returns the internal, protocol-level network chain from the state.
    pub fn get_network_state_name(
        &self,
        state_cid: Cid,
    ) -> anyhow::Result<crate::networks::StateNetworkName> {
        let init_act = self
            .get_actor(&init::ADDRESS.into(), state_cid)?
            .ok_or_else(|| Error::state("Init actor address could not be resolved"))?;
        Ok(
            State::load(self.blockstore(), init_act.code, init_act.state)?
                .into_network_name()
                .into(),
        )
    }

    /// Returns true if miner has been slashed or is considered invalid.
    pub fn is_miner_slashed(&self, addr: &Address, state_cid: &Cid) -> anyhow::Result<bool, Error> {
        let actor = self
            .get_actor(&Address::POWER_ACTOR, *state_cid)?
            .ok_or_else(|| Error::state("Power actor address could not be resolved"))?;

        let spas = power::State::load(self.blockstore(), actor.code, actor.state)?;

        Ok(spas.miner_power(self.blockstore(), addr)?.is_none())
    }

    /// Returns raw work address of a miner given the state root.
    pub fn get_miner_work_addr(&self, state_cid: Cid, addr: &Address) -> Result<Address, Error> {
        let state =
            StateTree::new_from_root(self.blockstore_owned(), &state_cid).map_err(Error::other)?;
        let ms: miner::State = state.get_actor_state_from_address(addr)?;
        let info = ms.info(self.blockstore()).map_err(|e| e.to_string())?;
        let addr = resolve_to_key_addr(&state, self.blockstore(), &info.worker())?;
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
                .miner_power(self.blockstore(), maddr)?
                .ok_or_else(|| Error::state(format!("Miner for address {maddr} not found")))?;

            let min_pow = spas.miner_nominal_power_meets_consensus_minimum(
                &self.chain_config().policy,
                self.blockstore(),
                maddr,
            )?;
            if min_pow {
                return Ok(Some((m_pow, t_pow)));
            }
        }

        Ok(None)
    }

    // Returns all sectors
    pub fn get_all_sectors(
        &self,
        addr: &Address,
        ts: &Tipset,
    ) -> anyhow::Result<Vec<SectorOnChainInfo>> {
        let actor = self
            .get_actor(addr, *ts.parent_state())?
            .ok_or_else(|| Error::state("Miner actor not found"))?;
        let state = miner::State::load(self.blockstore(), actor.code, actor.state)?;
        state.load_sectors_ext(self.blockstore(), None)
    }
}

impl<DB> StateManager<DB>
where
    DB: Blockstore + Send + Sync + 'static,
{
    /// Load the state of a tipset, including state root, message receipts
    pub async fn load_tipset_state(self: &Arc<Self>, ts: &Tipset) -> anyhow::Result<TipsetState> {
        if let Some(state) = self.cache.get_map(ts.key(), |et| et.into()) {
            Ok(state)
        } else {
            match self.chain_store().load_child_tipset(ts)? {
                Some(receipt_ts) => Ok(TipsetState {
                    state_root: *receipt_ts.parent_state(),
                    receipt_root: *receipt_ts.parent_message_receipts(),
                }),
                None => Ok(self.load_executed_tipset(ts).await?.into()),
            }
        }
    }

    /// Load an executed tipset, including state root, message receipts and events with caching.
    pub async fn load_executed_tipset(
        self: &Arc<Self>,
        ts: &Tipset,
    ) -> anyhow::Result<ExecutedTipset> {
        // validate the existence of state trees for post-chain-head-epoch tipsets in case chain head is reset(e.g. manually or via GC).
        if ts.epoch() >= self.heaviest_tipset().epoch()
            && let Some(cached) = self.cache.get(ts.key())
        {
            if StateTree::new_from_root(self.blockstore_owned(), &cached.state_root).is_ok() {
                return Ok(cached);
            } else {
                self.cache.remove(ts.key());
            }
        }
        self.cache
            .get_or_else(ts.key(), || async move {
                let receipt_ts = self.chain_store().load_child_tipset(ts)?;
                self.load_executed_tipset_inner(ts, receipt_ts.as_ref())
                    .await
            })
            .await
    }

    async fn load_executed_tipset_inner(
        self: &Arc<Self>,
        msg_ts: &Tipset,
        // when `msg_ts` is the current head, `receipt_ts` is `None`
        receipt_ts: Option<&Tipset>,
    ) -> anyhow::Result<ExecutedTipset> {
        if let Some(receipt_ts) = receipt_ts {
            anyhow::ensure!(
                msg_ts.key() == receipt_ts.parents(),
                "message tipset should be the parent of message receipt tipset"
            );
        }
        let mut recomputed = false;
        let (state_root, receipt_root, receipts) = match receipt_ts.and_then(|ts| {
            let receipt_root = *ts.parent_message_receipts();
            Receipt::get_receipts(self.cs.blockstore(), receipt_root)
                .ok()
                .map(|r| (*ts.parent_state(), receipt_root, r))
        }) {
            Some((state_root, receipt_root, receipts)) => (state_root, receipt_root, receipts),
            None => {
                let state_output = self
                    .compute_tipset_state(msg_ts.shallow_clone(), NO_CALLBACK, VMTrace::NotTraced)
                    .await?;
                recomputed = true;
                (
                    state_output.state_root,
                    state_output.receipt_root,
                    Receipt::get_receipts(self.cs.blockstore(), state_output.receipt_root)?,
                )
            }
        };

        let messages = self.chain_store().messages_for_tipset(msg_ts)?;
        anyhow::ensure!(
            messages.len() == receipts.len(),
            "mismatching message and receipt counts ({} messages, {} receipts)",
            messages.len(),
            receipts.len()
        );
        let mut executed_messages = Vec::with_capacity(messages.len());
        for (message, receipt) in messages.iter().cloned().zip(receipts) {
            let events = if let Some(events_root) = receipt.events_root() {
                Some(
                    match StampedEvent::get_events(self.cs.blockstore(), &events_root) {
                        Ok(events) => events,
                        Err(e) if recomputed => return Err(e),
                        Err(_) => {
                            self.compute_tipset_state(
                                msg_ts.shallow_clone(),
                                NO_CALLBACK,
                                VMTrace::NotTraced,
                            )
                            .await?;
                            recomputed = true;
                            StampedEvent::get_events(self.cs.blockstore(), &events_root)?
                        }
                    },
                )
            } else {
                None
            };
            executed_messages.push(ExecutedMessage {
                message,
                receipt,
                events,
            });
        }
        Ok(ExecutedTipset {
            state_root,
            receipt_root,
            executed_messages: Arc::new(executed_messages),
        })
    }

    /// Replays the given message and returns the result of executing the
    /// indicated message, assuming it was executed in the indicated tipset.
    pub async fn replay(self: &Arc<Self>, ts: Tipset, mcid: Cid) -> Result<ApiInvocResult, Error> {
        let this = Arc::clone(self);
        tokio::task::spawn_blocking(move || this.replay_blocking(ts, mcid)).await?
    }

    /// Blocking version of `replay`
    pub fn replay_blocking(
        self: &Arc<Self>,
        ts: Tipset,
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
                        duration: ctx.duration.as_nanos().clamp(0, u128::from(u64::MAX)) as u64,
                        gas_cost: MessageGasCost::new(ctx.message.message(), ctx.apply_ret)?,
                        execution_trace: structured::parse_events(ctx.apply_ret.exec_trace())
                            .unwrap_or_default(),
                    });
                    anyhow::bail!(REPLAY_HALT);
                }
                _ => Ok(()), // ignored
            }
        };
        let result = self.compute_tipset_state_blocking(ts, Some(callback), VMTrace::Traced);
        if let Err(error_message) = result
            && error_message.to_string() != REPLAY_HALT
        {
            return Err(Error::Other(format!(
                "unexpected error during execution : {error_message:}"
            )));
        }
        api_invoc_result.ok_or_else(|| Error::Other("failed to replay".into()))
    }

    /// Replays a tipset up to a target message, capturing the state root before
    /// and after execution.
    pub async fn replay_for_prestate(
        self: &Arc<Self>,
        ts: Tipset,
        target_message_cid: Cid,
    ) -> Result<(Cid, ApiInvocResult, Cid), Error> {
        let this = Arc::clone(self);
        tokio::task::spawn_blocking(move || {
            this.replay_for_prestate_blocking(ts, target_message_cid)
        })
        .await
        .map_err(|e| Error::Other(format!("{e}")))?
    }

    fn replay_for_prestate_blocking(
        self: &Arc<Self>,
        ts: Tipset,
        target_msg_cid: Cid,
    ) -> Result<(Cid, ApiInvocResult, Cid), Error> {
        if ts.epoch() == 0 {
            return Err(Error::Other(
                "cannot trace messages in the genesis block".into(),
            ));
        }

        let genesis_timestamp = self.chain_store().genesis_block_header().timestamp;
        let exec = TipsetExecutor::new(
            self.chain_index().shallow_clone(),
            self.chain_config().shallow_clone(),
            self.beacon_schedule().shallow_clone(),
            &self.engine,
            ts.shallow_clone(),
        );
        let mut no_cb = NO_CALLBACK;
        let (parent_state, epoch, block_messages) =
            exec.prepare_parent_state(genesis_timestamp, VMTrace::NotTraced, &mut no_cb)?;

        Ok(stacker::grow(64 << 20, || {
            let mut vm =
                exec.create_vm(parent_state, epoch, ts.min_timestamp(), VMTrace::NotTraced)?;
            let mut processed = ahash::HashSet::default();

            for block in block_messages.iter() {
                let mut penalty = TokenAmount::zero();
                let mut gas_reward = TokenAmount::zero();

                for msg in block.messages.iter() {
                    let cid = msg.cid();
                    if processed.contains(&cid) {
                        continue;
                    }

                    processed.insert(cid);

                    if cid == target_msg_cid {
                        let pre_root = vm.flush()?;
                        let mut traced_vm =
                            exec.create_vm(pre_root, epoch, ts.min_timestamp(), VMTrace::Traced)?;
                        let (ret, duration) = traced_vm.apply_message(msg)?;
                        let post_root = traced_vm.flush()?;

                        return Ok((
                            pre_root,
                            ApiInvocResult {
                                msg_cid: cid,
                                msg: msg.message().clone(),
                                msg_rct: Some(ret.msg_receipt()),
                                error: ret.failure_info().unwrap_or_default(),
                                duration: duration.as_nanos().clamp(0, u128::from(u64::MAX)) as u64,
                                gas_cost: MessageGasCost::default(),
                                execution_trace: structured::parse_events(ret.exec_trace())
                                    .unwrap_or_default(),
                            },
                            post_root,
                        ));
                    }

                    let (ret, _) = vm.apply_message(msg)?;
                    gas_reward += ret.miner_tip();
                    penalty += ret.penalty();
                }

                if let Some(rew_msg) =
                    vm.reward_message(epoch, block.miner, block.win_count, penalty, gas_reward)?
                {
                    let (ret, _) = vm.apply_implicit_message(&rew_msg)?;
                    if let Some(err) = ret.failure_info() {
                        bail!(
                            "failed to apply reward message for miner {}: {err}",
                            block.miner
                        );
                    }

                    // This is more of a sanity check, this should not be able to be hit.
                    if !ret.msg_receipt().exit_code().is_success() {
                        bail!(
                            "reward application message failed (exit: {:?})",
                            ret.msg_receipt().exit_code()
                        );
                    }
                }
            }

            bail!("message {target_msg_cid} not found in tipset")
        })?)
    }

    /// Checks the eligibility of the miner. This is used in the validation that
    /// a block's miner has the requirements to mine a block.
    pub fn eligible_to_mine(
        &self,
        address: &Address,
        base_tipset: &Tipset,
        lookback_tipset: &Tipset,
    ) -> anyhow::Result<bool, Error> {
        let hmp =
            self.miner_has_min_power(&self.chain_config().policy, address, lookback_tipset)?;
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
    pub async fn compute_tipset_state(
        self: &Arc<Self>,
        tipset: Tipset,
        callback: Option<impl FnMut(MessageCallbackCtx<'_>) -> anyhow::Result<()> + Send + 'static>,
        enable_tracing: VMTrace,
    ) -> Result<ExecutedTipset, Error> {
        let this = Arc::clone(self);
        tokio::task::spawn_blocking(move || {
            this.compute_tipset_state_blocking(tipset, callback, enable_tracing)
        })
        .await?
    }

    /// Blocking version of `compute_tipset_state`
    pub fn compute_tipset_state_blocking(
        &self,
        tipset: Tipset,
        callback: Option<impl FnMut(MessageCallbackCtx<'_>) -> anyhow::Result<()>>,
        enable_tracing: VMTrace,
    ) -> Result<ExecutedTipset, Error> {
        let epoch = tipset.epoch();
        let has_callback = callback.is_some();
        info!(
            "Evaluating tipset: EPOCH={epoch}, blocks={}, tsk={}",
            tipset.len(),
            tipset.key(),
        );
        Ok(apply_block_messages(
            self.chain_store().genesis_block_header().timestamp,
            self.chain_index().shallow_clone(),
            self.chain_config().shallow_clone(),
            self.beacon_schedule().shallow_clone(),
            &self.engine,
            tipset,
            callback,
            enable_tracing,
        )
        .map_err(|e| {
            if has_callback {
                e
            } else {
                e.context(format!("Failed to compute tipset state@{epoch}"))
            }
        })?)
    }

    #[instrument(skip_all)]
    pub async fn compute_state(
        self: &Arc<Self>,
        height: ChainEpoch,
        messages: Vec<Message>,
        tipset: Tipset,
        callback: Option<impl FnMut(MessageCallbackCtx<'_>) -> anyhow::Result<()> + Send + 'static>,
        enable_tracing: VMTrace,
    ) -> Result<ExecutedTipset, Error> {
        let this = Arc::clone(self);
        tokio::task::spawn_blocking(move || {
            this.compute_state_blocking(height, messages, tipset, callback, enable_tracing)
        })
        .await?
    }

    /// Blocking version of `compute_state`
    #[tracing::instrument(skip_all)]
    pub fn compute_state_blocking(
        &self,
        height: ChainEpoch,
        messages: Vec<Message>,
        tipset: Tipset,
        callback: Option<impl FnMut(MessageCallbackCtx<'_>) -> anyhow::Result<()>>,
        enable_tracing: VMTrace,
    ) -> Result<ExecutedTipset, Error> {
        Ok(compute_state(
            height,
            messages,
            tipset,
            self.chain_store().genesis_block_header().timestamp,
            self.chain_index().shallow_clone(),
            self.chain_config().shallow_clone(),
            self.beacon_schedule().shallow_clone(),
            &self.engine,
            callback,
            enable_tracing,
        )?)
    }

    pub async fn miner_get_base_info(
        self: &Arc<Self>,
        beacon_schedule: &BeaconSchedule,
        tipset: Tipset,
        addr: Address,
        epoch: ChainEpoch,
    ) -> anyhow::Result<Option<MiningBaseInfo>> {
        let prev_beacon = self
            .chain_store()
            .chain_index()
            .latest_beacon_entry(tipset.clone())?;

        let entries: Vec<BeaconEntry> = beacon_schedule
            .beacon_entries_for_block(
                self.chain_config().network_version(epoch),
                epoch,
                tipset.epoch(),
                &prev_beacon,
            )
            .await?;

        let base = entries.last().unwrap_or(&prev_beacon);

        let (lb_tipset, lb_state_root) = ChainStore::get_lookback_tipset_for_round(
            self.chain_index(),
            self.chain_config(),
            &tipset,
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

        let network_version = self.chain_config().network_version(tipset.epoch());
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
            .resolve_to_deterministic_address(info.worker, &tipset)
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

        ps.miner_nominal_power_meets_consensus_minimum(policy, self.blockstore(), addr)
    }

    /// Validates all tipsets at epoch `start..=end` behind the heaviest tipset.
    ///
    /// Tipsets are processed sequentially. The compute-intensive work inside each
    /// tipset (`bellperson` proof verification, FVM batch seal verification, etc.)
    /// is already heavily rayon-parallelized. Parallelizing the outer loop actually introduces
    /// some issues due to locks in the aforementioned crates. So don't do it.
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
    #[tracing::instrument(skip(self))]
    pub fn validate_range(&self, epochs: RangeInclusive<i64>) -> anyhow::Result<()> {
        let heaviest = self.heaviest_tipset();
        let heaviest_epoch = heaviest.epoch();
        let end = self.chain_index().load_required_tipset_by_height(
            *epochs.end(),
            heaviest,
            ResolveNullTipset::TakeOlder,
        ).with_context(|| {
            format!(
        "couldn't get a tipset at height {} behind heaviest tipset at height {heaviest_epoch}",
        *epochs.end(),
    )})?;

        // lookup tipset parents as we go along, iterating DOWN from `end`
        let tipsets = end
            .chain(self.blockstore())
            .take_while(|ts| ts.epoch() >= *epochs.start());

        self.validate_tipsets(tipsets)
    }

    pub fn validate_tipsets<T>(&self, tipsets: T) -> anyhow::Result<()>
    where
        T: Iterator<Item = Tipset> + Send,
    {
        let genesis_timestamp = self.chain_store().genesis_block_header().timestamp;
        validate_tipsets(
            genesis_timestamp,
            self.chain_index(),
            self.chain_config(),
            self.beacon_schedule(),
            &self.engine,
            tipsets,
        )
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
                        duration: ctx.duration.as_nanos().clamp(0, u128::from(u64::MAX)) as u64,
                        gas_cost: MessageGasCost::new(ctx.message.message(), ctx.apply_ret)?,
                        execution_trace: structured::parse_events(ctx.apply_ret.exec_trace())
                            .unwrap_or_default(),
                    });
                    Ok(())
                }
                _ => Ok(()), // ignored
            }
        };

        let ExecutedTipset { state_root, .. } = apply_block_messages(
            genesis_timestamp,
            self.chain_index().shallow_clone(),
            self.chain_config().shallow_clone(),
            self.beacon_schedule().shallow_clone(),
            &self.engine,
            tipset.shallow_clone(),
            Some(callback),
            VMTrace::Traced,
        )?;

        Ok((state_root, invoc_trace))
    }
}

pub fn validate_tipsets<DB, T>(
    genesis_timestamp: u64,
    chain_index: &ChainIndex<DB>,
    chain_config: &Arc<ChainConfig>,
    beacon: &Arc<BeaconSchedule>,
    engine: &MultiEngine,
    tipsets: T,
) -> anyhow::Result<()>
where
    DB: Blockstore + Send + Sync + 'static,
    T: Iterator<Item = Tipset> + Send,
{
    // Validate one tipset at a time. Parallelizing the outer loop across tipsets
    // might wedge the global rayon pool.
    // Sequential outer iteration leaves the entire rayon pool free for that
    // already-rich inner parallelism.
    for (child, parent) in tipsets.tuple_windows() {
        info!(height = parent.epoch(), "compute parent state");
        let ExecutedTipset {
            state_root: actual_state,
            receipt_root: actual_receipt,
            ..
        } = apply_block_messages(
            genesis_timestamp,
            chain_index.shallow_clone(),
            chain_config.shallow_clone(),
            beacon.shallow_clone(),
            engine,
            parent,
            NO_CALLBACK,
            VMTrace::NotTraced,
        )
        .context("couldn't compute tipset state")?;
        let expected_receipt = child.min_ticket_block().message_receipts;
        let expected_state = child.parent_state();
        if (expected_state, expected_receipt) != (&actual_state, actual_receipt) {
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
    Ok(())
}

/// Shared context for creating VMs and preparing tipset state.
///
/// Encapsulates randomness source, genesis info, VM construction,
/// null-epoch cron handling, and state migrations.
struct TipsetExecutor<'a, DB: Blockstore + Send + Sync + 'static> {
    tipset: Tipset,
    rand: ChainRand<DB>,
    chain_config: Arc<ChainConfig>,
    chain_index: ChainIndex<DB>,
    genesis_info: GenesisInfo,
    engine: &'a MultiEngine,
}

impl<'a, DB: Blockstore + Send + Sync + 'static> TipsetExecutor<'a, DB> {
    fn new(
        chain_index: ChainIndex<DB>,
        chain_config: Arc<ChainConfig>,
        beacon: Arc<BeaconSchedule>,
        engine: &'a MultiEngine,
        tipset: Tipset,
    ) -> Self {
        let rand = ChainRand::new(
            chain_config.shallow_clone(),
            tipset.shallow_clone(),
            chain_index.shallow_clone(),
            beacon,
        );
        let genesis_info = GenesisInfo::from_chain_config(chain_config.shallow_clone());
        Self {
            tipset,
            rand,
            chain_config,
            chain_index,
            genesis_info,
            engine,
        }
    }

    fn create_vm(
        &self,
        state_root: Cid,
        epoch: ChainEpoch,
        timestamp: u64,
        trace: VMTrace,
    ) -> anyhow::Result<VM<DB>> {
        let circ_supply = self.genesis_info.get_vm_circulating_supply(
            epoch,
            self.chain_index.db(),
            &state_root,
        )?;
        VM::new(
            ExecutionContext {
                heaviest_tipset: self.tipset.shallow_clone(),
                state_tree_root: state_root,
                epoch,
                rand: Box::new(self.rand.shallow_clone()),
                base_fee: self.tipset.min_ticket_block().parent_base_fee.clone(),
                circ_supply,
                chain_config: self.chain_config.shallow_clone(),
                chain_index: self.chain_index.shallow_clone(),
                timestamp,
            },
            self.engine,
            trace,
        )
    }

    /// Produces the state root ready for message execution by running
    /// null-epoch `crons` and any pending state migrations.
    fn prepare_parent_state<F>(
        &self,
        genesis_timestamp: u64,
        null_epoch_trace: VMTrace,
        cron_callback: &mut Option<F>,
    ) -> anyhow::Result<(Cid, ChainEpoch, Vec<BlockMessages>)>
    where
        F: FnMut(MessageCallbackCtx<'_>) -> anyhow::Result<()>,
    {
        use crate::shim::clock::EPOCH_DURATION_SECONDS;

        let mut parent_state = *self.tipset.parent_state();
        let parent_epoch = self
            .chain_index
            .load_required_tipset(self.tipset.parents())?
            .epoch();
        let epoch = self.tipset.epoch();

        for epoch_i in parent_epoch..epoch {
            if epoch_i > parent_epoch {
                let timestamp = genesis_timestamp + ((EPOCH_DURATION_SECONDS * epoch_i) as u64);
                parent_state = stacker::grow(64 << 20, || -> anyhow::Result<Cid> {
                    let mut vm =
                        self.create_vm(parent_state, epoch_i, timestamp, null_epoch_trace)?;
                    if let Err(e) = vm.run_cron(epoch_i, cron_callback.as_mut()) {
                        error!("Beginning of epoch cron failed to run: {e:#}");
                        return Err(e);
                    }
                    vm.flush()
                })?;
            }
            if let Some(new_state) = run_state_migrations(
                epoch_i,
                &self.chain_config,
                self.chain_index.db(),
                &parent_state,
            )? {
                parent_state = new_state;
            }
        }

        let block_messages = BlockMessages::for_tipset(self.chain_index.db(), &self.tipset)?;
        Ok((parent_state, epoch, block_messages))
    }
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
    chain_index: ChainIndex<DB>,
    chain_config: Arc<ChainConfig>,
    beacon: Arc<BeaconSchedule>,
    engine: &MultiEngine,
    tipset: Tipset,
    mut callback: Option<impl FnMut(MessageCallbackCtx<'_>) -> anyhow::Result<()>>,
    enable_tracing: VMTrace,
) -> anyhow::Result<ExecutedTipset>
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
        return Ok(ExecutedTipset {
            state_root: *tipset.parent_state(),
            receipt_root: message_receipts,
            executed_messages: vec![].into(),
        });
    }

    let exec = TipsetExecutor::new(
        chain_index.shallow_clone(),
        chain_config,
        beacon,
        engine,
        tipset.shallow_clone(),
    );

    // step 2: running cron for any null-tipsets
    // step 3: run migrations
    let (parent_state, epoch, block_messages) =
        exec.prepare_parent_state(genesis_timestamp, enable_tracing, &mut callback)?;

    // FVM requires a stack size of 64MiB. The alternative is to use `ThreadedExecutor` from
    // FVM, but that introduces some constraints, and possible deadlocks.
    stacker::grow(64 << 20, || -> anyhow::Result<ExecutedTipset> {
        let mut vm = exec.create_vm(parent_state, epoch, tipset.min_timestamp(), enable_tracing)?;

        // step 4: apply tipset messages
        let (receipts, events, events_roots) =
            vm.apply_block_messages(&block_messages, epoch, callback)?;

        // step 5: construct receipt root from receipts
        let receipt_root = Amtv0::new_from_iter(chain_index.db(), receipts.iter())?;

        // step 6: store events AMTs in the blockstore
        for (events, events_root) in events.iter().zip(events_roots.iter()) {
            if let Some(events) = events {
                let event_root =
                    events_root.context("events root should be present when events present")?;
                // Store the events AMT - the root CID should match the one computed by FVM
                let derived_event_root = Amt::new_from_iter_with_bit_width(
                    chain_index.db(),
                    EVENTS_AMT_BITWIDTH,
                    events.iter(),
                )
                .map_err(|e| Error::Other(format!("failed to store events AMT: {e}")))?;

                // Verify the stored root matches the FVM-computed root
                ensure!(
                    derived_event_root == event_root,
                    "Events AMT root mismatch: derived={derived_event_root}, actual={event_root}."
                );
            }
        }

        let state_root = vm.flush()?;

        // Update executed tipset cache
        let messages: Vec<ChainMessage> = block_messages
            .into_iter()
            .flat_map(|bm| bm.messages)
            .collect_vec();
        anyhow::ensure!(
            messages.len() == receipts.len() && messages.len() == events.len(),
            "length of messages, receipts, and events should match",
        );
        Ok(ExecutedTipset {
            state_root,
            receipt_root,
            executed_messages: messages
                .into_iter()
                .zip(receipts)
                .zip(events)
                .map(|((message, receipt), events)| ExecutedMessage {
                    message,
                    receipt,
                    events,
                })
                .collect_vec()
                .into(),
        })
    })
}

#[allow(clippy::too_many_arguments)]
pub fn compute_state<DB>(
    _height: ChainEpoch,
    messages: Vec<Message>,
    tipset: Tipset,
    genesis_timestamp: u64,
    chain_index: ChainIndex<DB>,
    chain_config: Arc<ChainConfig>,
    beacon: Arc<BeaconSchedule>,
    engine: &MultiEngine,
    callback: Option<impl FnMut(MessageCallbackCtx<'_>) -> anyhow::Result<()>>,
    enable_tracing: VMTrace,
) -> anyhow::Result<ExecutedTipset>
where
    DB: Blockstore + Send + Sync + 'static,
{
    if !messages.is_empty() {
        anyhow::bail!("Applying messages is not yet implemented.");
    }

    let output = apply_block_messages(
        genesis_timestamp,
        chain_index,
        chain_config,
        beacon,
        engine,
        tipset,
        callback,
        enable_tracing,
    )?;

    Ok(output)
}

/// Controls whether the VM should flush its state after execution
#[derive(Debug, Copy, Clone, Default)]
pub enum VMFlush {
    Flush,
    #[default]
    Skip,
}
