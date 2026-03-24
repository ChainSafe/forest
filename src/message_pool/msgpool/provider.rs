// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::blocks::{CachingBlockHeader, Tipset, TipsetKey};
use crate::chain::{ChainStore, HeadChanges};
use crate::message::{ChainMessage, SignedMessage};
use crate::message_pool::errors::Error;
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
use crate::utils::db::CborStoreExt;
use auto_impl::auto_impl;
use cid::Cid;
use fvm_ipld_blockstore::Blockstore;
use tokio::sync::broadcast;

/// Provider Trait. This trait will be used by the message pool to interact with
/// some medium in order to do the operations that are listed below that are
/// required for the message pool.
#[auto_impl(Arc)]
pub trait Provider {
    /// Update `Mpool`'s `cur_tipset` whenever there is a change to the provider
    fn subscribe_head_changes(&self) -> broadcast::Receiver<HeadChanges>;
    /// Get the heaviest Tipset in the provider
    fn get_heaviest_tipset(&self) -> Tipset;
    /// Add a message to the `MpoolProvider`, return either Cid or Error
    /// depending on successful put
    fn put_message(&self, msg: &ChainMessage) -> Result<Cid, Error>;
    /// Return state actor for given address given the tipset that the a temp
    /// `StateTree` will be rooted at. Return `ActorState` or Error
    /// depending on whether or not `ActorState` is found
    fn get_actor_after(&self, addr: &Address, ts: &Tipset) -> Result<ActorState, Error>;
    /// Next assignable sequence: parent-state actor sequence merged with messages in `ts` blocks.
    fn get_state_nonce(&self, addr: &Address, ts: &Tipset) -> Result<u64, Error>;
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

impl<DB: Blockstore> Provider for ChainStore<DB> {
    fn subscribe_head_changes(&self) -> broadcast::Receiver<HeadChanges> {
        self.subscribe_head_changes()
    }

    fn get_heaviest_tipset(&self) -> Tipset {
        self.heaviest_tipset()
    }

    fn put_message(&self, msg: &ChainMessage) -> Result<Cid, Error> {
        let cid = self
            .blockstore()
            .put_cbor_default(msg)
            .map_err(|err| Error::Other(err.to_string()))?;
        Ok(cid)
    }

    fn get_actor_after(&self, addr: &Address, ts: &Tipset) -> Result<ActorState, Error> {
        let state = StateTree::new_from_root(self.blockstore().clone(), ts.parent_state())
            .map_err(|e| Error::Other(e.to_string()))?;
        Ok(state.get_required_actor(addr)?)
    }

    fn get_state_nonce(&self, addr: &Address, ts: &Tipset) -> Result<u64, Error> {
        let state = StateTree::new_from_root(self.blockstore().clone(), ts.parent_state())
            .map_err(|e| Error::Other(e.to_string()))?;
        let Some(actor) = state.get_actor(addr)? else {
            return Ok(0);
        };
        let mut next = actor.sequence;
        let sender_id = state
            .lookup_required_id(addr)
            .map_err(|e| Error::Other(e.to_string()))?;
        for bh in ts.block_headers() {
            let (unsigned, signed) =
                crate::chain::block_messages(self.blockstore(), bh).map_err(Error::from)?;
            for m in &signed {
                if state
                    .lookup_id(&m.message().from)
                    .map_err(|e| Error::Other(e.to_string()))?
                    == Some(sender_id)
                {
                    next = next.max(m.message().sequence + 1);
                }
            }
            for m in &unsigned {
                if state
                    .lookup_id(&m.from)
                    .map_err(|e| Error::Other(e.to_string()))?
                    == Some(sender_id)
                {
                    next = next.max(m.sequence + 1);
                }
            }
        }
        Ok(next)
    }

    fn messages_for_block(
        &self,
        h: &CachingBlockHeader,
    ) -> Result<(Vec<Message>, Vec<SignedMessage>), Error> {
        crate::chain::block_messages(self.blockstore(), h).map_err(|err| err.into())
    }

    fn load_tipset(&self, tsk: &TipsetKey) -> Result<Tipset, Error> {
        Ok(self.chain_index().load_required_tipset(tsk)?)
    }

    fn chain_compute_base_fee(&self, ts: &Tipset) -> Result<TokenAmount, Error> {
        let smoke_height = self.chain_config().epoch(Height::Smoke);
        let xxx_height = self.chain_config().epoch(Height::Xxx);
        crate::chain::compute_base_fee(self.blockstore(), ts, smoke_height, xxx_height)
            .map_err(|err| err.into())
    }
}
