// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::sync::Arc;

use crate::blocks::{CachingBlockHeader, Tipset, TipsetKey};
use crate::chain::HeadChange;
use crate::message::{ChainMessage, SignedMessage};
use crate::message_pool::msg_pool::{
    MAX_ACTOR_PENDING_MESSAGES, MAX_UNTRUSTED_ACTOR_PENDING_MESSAGES,
};
use crate::networks::Height;
use crate::shim::{
    address::Address,
    econ::TokenAmount,
    message::Message,
    state_tree::{ActorState, StateTree},
};
use crate::state_manager::StateManager;
use crate::utils::db::CborStoreExt;
use async_trait::async_trait;
use cid::Cid;
use fvm_ipld_blockstore::Blockstore;
use tokio::sync::broadcast::{Receiver as Subscriber, Sender as Publisher};

use crate::message_pool::errors::Error;

/// Provider Trait. This trait will be used by the message pool to interact with
/// some medium in order to do the operations that are listed below that are
/// required for the message pool.
#[async_trait]
pub trait Provider {
    /// Update `Mpool`'s `cur_tipset` whenever there is a change to the provider
    fn subscribe_head_changes(&self) -> Subscriber<HeadChange>;
    /// Get the heaviest Tipset in the provider
    fn get_heaviest_tipset(&self) -> Tipset;
    /// Add a message to the `MpoolProvider`, return either Cid or Error
    /// depending on successful put
    fn put_message(&self, msg: &ChainMessage) -> Result<Cid, Error>;
    /// Return state actor for given address given the tipset that the a temp
    /// `StateTree` will be rooted at. Return `ActorState` or Error
    /// depending on whether or not `ActorState` is found
    fn get_actor_after(&self, addr: &Address, ts: &Tipset) -> Result<ActorState, Error>;
    /// Return the signed messages for given block header
    fn messages_for_block(
        &self,
        h: &CachingBlockHeader,
    ) -> Result<(Vec<Message>, Vec<SignedMessage>), Error>;
    /// Return a tipset given the tipset keys from the `ChainStore`
    fn load_tipset(&self, tsk: &TipsetKey) -> Result<Tipset, Error>;
    /// Computes the base fee
    fn chain_compute_base_fee(&self, ts: &Tipset) -> Result<TokenAmount, Error>;
    // Get max number of messages per actor in the pool
    fn max_actor_pending_messages(&self) -> u64 {
        MAX_ACTOR_PENDING_MESSAGES
    }
    // Get max number of messages per actor in the pool for untrusted sources
    fn max_untrusted_actor_pending_messages(&self) -> u64 {
        MAX_UNTRUSTED_ACTOR_PENDING_MESSAGES
    }
}

/// This is the default Provider implementation that will be used for the
/// `mpool` RPC.
#[derive(derive_more::Constructor)]
pub struct MpoolRpcProvider<DB> {
    subscriber: Publisher<HeadChange>,
    sm: Arc<StateManager<DB>>,
}

#[async_trait]
impl<DB> Provider for MpoolRpcProvider<DB>
where
    DB: Blockstore + Sync + Send + 'static,
{
    fn subscribe_head_changes(&self) -> Subscriber<HeadChange> {
        self.subscriber.subscribe()
    }

    fn get_heaviest_tipset(&self) -> Tipset {
        self.sm.chain_store().heaviest_tipset()
    }

    fn put_message(&self, msg: &ChainMessage) -> Result<Cid, Error> {
        let cid = self
            .sm
            .blockstore()
            .put_cbor_default(msg)
            .map_err(|err| Error::Other(err.to_string()))?;
        Ok(cid)
    }

    fn get_actor_after(&self, addr: &Address, ts: &Tipset) -> Result<ActorState, Error> {
        let state = StateTree::new_from_root(self.sm.blockstore_owned(), ts.parent_state())
            .map_err(|e| Error::Other(e.to_string()))?;
        Ok(state.get_required_actor(addr)?)
    }

    fn messages_for_block(
        &self,
        h: &CachingBlockHeader,
    ) -> Result<(Vec<Message>, Vec<SignedMessage>), Error> {
        crate::chain::block_messages(self.sm.blockstore(), h).map_err(|err| err.into())
    }

    fn load_tipset(&self, tsk: &TipsetKey) -> Result<Tipset, Error> {
        Ok(self
            .sm
            .chain_store()
            .chain_index()
            .load_required_tipset(tsk)?)
    }

    fn chain_compute_base_fee(&self, ts: &Tipset) -> Result<TokenAmount, Error> {
        let smoke_height = self.sm.chain_config().epoch(Height::Smoke);
        crate::chain::compute_base_fee(self.sm.blockstore(), ts, smoke_height)
            .map_err(|err| err.into())
    }
}
