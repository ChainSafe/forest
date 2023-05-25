// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::sync::Arc;

use async_trait::async_trait;
use cid::{multihash::Code::Blake2b256, Cid};
use forest_blocks::{BlockHeader, Tipset, TipsetKeys};
use forest_chain::HeadChange;
use forest_message::{ChainMessage, SignedMessage};
use forest_networks::Height;
use forest_shim::{
    address::Address,
    econ::TokenAmount,
    message::Message,
    state_tree::{ActorState, StateTree},
};
use forest_state_manager::StateManager;
use forest_utils::db::BlockstoreExt;
use fvm_ipld_blockstore::Blockstore;
use tokio::sync::broadcast::{Receiver as Subscriber, Sender as Publisher};

use crate::errors::Error;

/// Provider Trait. This trait will be used by the message pool to interact with
/// some medium in order to do the operations that are listed below that are
/// required for the message pool.
#[async_trait]
pub trait Provider {
    /// Update `Mpool`'s `cur_tipset` whenever there is a change to the provider
    fn subscribe_head_changes(&self) -> Subscriber<HeadChange>;
    /// Get the heaviest Tipset in the provider
    fn get_heaviest_tipset(&self) -> Arc<Tipset>;
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
        h: &BlockHeader,
    ) -> Result<(Vec<Message>, Vec<SignedMessage>), Error>;
    /// Return all messages for a tipset
    fn messages_for_tipset(&self, h: &Tipset) -> Result<Vec<ChainMessage>, Error>;
    /// Return a tipset given the tipset keys from the `ChainStore`
    fn load_tipset(&self, tsk: &TipsetKeys) -> Result<Arc<Tipset>, Error>;
    /// Computes the base fee
    fn chain_compute_base_fee(&self, ts: &Tipset) -> Result<TokenAmount, Error>;
}

/// This is the default Provider implementation that will be used for the
/// `mpool` RPC.
pub struct MpoolRpcProvider<DB> {
    subscriber: Publisher<HeadChange>,
    sm: Arc<StateManager<DB>>,
}

impl<DB> MpoolRpcProvider<DB>
where
    DB: Blockstore + Clone + Sync + Send,
{
    pub fn new(subscriber: Publisher<HeadChange>, sm: Arc<StateManager<DB>>) -> Self
    where
        DB: Blockstore + Clone,
    {
        MpoolRpcProvider { subscriber, sm }
    }
}

#[async_trait]
impl<DB> Provider for MpoolRpcProvider<DB>
where
    DB: Blockstore + Clone + Sync + Send + 'static,
{
    fn subscribe_head_changes(&self) -> Subscriber<HeadChange> {
        self.subscriber.subscribe()
    }

    fn get_heaviest_tipset(&self) -> Arc<Tipset> {
        self.sm.chain_store().heaviest_tipset()
    }

    fn put_message(&self, msg: &ChainMessage) -> Result<Cid, Error> {
        let cid = self
            .sm
            .blockstore()
            .put_obj(msg, Blake2b256)
            .map_err(|err| Error::Other(err.to_string()))?;
        Ok(cid)
    }

    fn get_actor_after(&self, addr: &Address, ts: &Tipset) -> Result<ActorState, Error> {
        let state = StateTree::new_from_root(self.sm.blockstore(), ts.parent_state())
            .map_err(|e| Error::Other(e.to_string()))?;

        let actor = state
            .get_actor(addr)
            .map_err(|e| Error::Other(e.to_string()))?;
        actor.ok_or_else(|| Error::Other("No actor state".to_owned()))
    }

    fn messages_for_block(
        &self,
        h: &BlockHeader,
    ) -> Result<(Vec<Message>, Vec<SignedMessage>), Error> {
        forest_chain::block_messages(self.sm.blockstore(), h).map_err(|err| err.into())
    }

    fn messages_for_tipset(&self, h: &Tipset) -> Result<Vec<ChainMessage>, Error> {
        Ok(self.sm.chain_store().messages_for_tipset(h)?)
    }

    fn load_tipset(&self, tsk: &TipsetKeys) -> Result<Arc<Tipset>, Error> {
        Ok(self.sm.chain_store().tipset_from_keys(tsk)?)
    }
    fn chain_compute_base_fee(&self, ts: &Tipset) -> Result<TokenAmount, Error> {
        let smoke_height = self.sm.chain_config().epoch(Height::Smoke);
        forest_chain::compute_base_fee(self.sm.blockstore(), ts, smoke_height)
            .map_err(|err| err.into())
            .map(Into::into)
    }
}
