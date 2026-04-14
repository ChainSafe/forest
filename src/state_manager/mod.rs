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
mod execution;
mod message_search;
mod message_simulation;
mod mining;
mod state_computation;
pub mod utils;

pub use self::errors::*;
pub use self::state_computation::{apply_block_messages, validate_tipsets};

use crate::beacon::BeaconSchedule;
use crate::blocks::Tipset;
use crate::chain::{
    ChainStore,
    index::{ChainIndex, ResolveNullTipset},
};
use crate::interpreter::{MessageCallbackCtx, resolve_to_key_addr};
use crate::lotus_json::{LotusJson, lotus_json_with_self};
use crate::message::ChainMessage;
use crate::networks::ChainConfig;
use crate::rpc::types::SectorOnChainInfo;
use crate::shim::actors::init::{self, State};
use crate::shim::actors::miner::ext::MinerStateExt as _;
use crate::shim::actors::*;
use crate::shim::executor::{Receipt, StampedEvent};
use crate::shim::{
    address::Address,
    clock::ChainEpoch,
    econ::TokenAmount,
    machine::{GLOBAL_MULTI_ENGINE, MultiEngine},
    state_tree::{ActorState, StateTree},
    version::NetworkVersion,
};
use crate::state_manager::cache::TipsetStateCache;
use crate::utils::ShallowClone as _;
use crate::utils::get_size::{GetSize, vec_heap_size_helper};
use anyhow::Context as _;
use chain_rand::ChainRand;
use cid::Cid;
use fvm_ipld_blockstore::Blockstore;
use nonzero_ext::nonzero;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{num::NonZeroUsize, sync::Arc};

const DEFAULT_TIPSET_CACHE_SIZE: NonZeroUsize = nonzero!(1024usize);
pub(crate) const EVENTS_AMT_BITWIDTH: u32 = 5;

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
    cs: Arc<ChainStore<DB>>,
    cache: TipsetStateCache<ExecutedTipset>,
    beacon: Arc<BeaconSchedule>,
    engine: Arc<MultiEngine>,
}

#[allow(clippy::type_complexity)]
pub const NO_CALLBACK: Option<fn(MessageCallbackCtx<'_>) -> anyhow::Result<()>> = None;

/// Controls whether the VM should flush its state after execution
#[derive(Debug, Copy, Clone, Default)]
pub enum VMFlush {
    Flush,
    #[default]
    Skip,
}

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
            cache: TipsetStateCache::new("executed_tipset"),
            beacon,
            engine,
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
                let target_head = self.chain_index().tipset_by_height(
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

    pub(in crate::state_manager) fn engine(&self) -> &Arc<MultiEngine> {
        &self.engine
    }

    pub(in crate::state_manager) fn tipset_state_cache(
        &self,
    ) -> &TipsetStateCache<ExecutedTipset> {
        &self.cache
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
            self.beacon_schedule().shallow_clone(),
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
