// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::errors::Error;
use address::Address;
use async_std::sync::Arc;
use async_trait::async_trait;
use blocks::BlockHeader;
use blocks::Tipset;
use blocks::TipsetKeys;
use blockstore::BlockStore;
use chain::HeadChange;
use cid::Cid;
use cid::Code::Blake2b256;
use message::{ChainMessage, SignedMessage, UnsignedMessage};
use num_bigint::BigInt;
use state_manager::StateManager;
use state_tree::StateTree;
use tokio::sync::broadcast::{Receiver as Subscriber, Sender as Publisher};
use types::verifier::ProofVerifier;
use vm::ActorState;

/// Provider Trait. This trait will be used by the messagepool to interact with some medium in order to do
/// the operations that are listed below that are required for the messagepool.
#[async_trait]
pub trait Provider {
    /// Update Mpool's cur_tipset whenever there is a chnge to the provider
    async fn subscribe_head_changes(&mut self) -> Subscriber<HeadChange>;
    /// Get the heaviest Tipset in the provider
    async fn get_heaviest_tipset(&mut self) -> Option<Arc<Tipset>>;
    /// Add a message to the MpoolProvider, return either Cid or Error depending on successful put
    fn put_message(&self, msg: &ChainMessage) -> Result<Cid, Error>;
    /// Return state actor for given address given the tipset that the a temp StateTree will be rooted
    /// at. Return ActorState or Error depending on whether or not ActorState is found
    fn get_actor_after(&self, addr: &Address, ts: &Tipset) -> Result<ActorState, Error>;
    /// Return the signed messages for given blockheader
    fn messages_for_block(
        &self,
        h: &BlockHeader,
    ) -> Result<(Vec<UnsignedMessage>, Vec<SignedMessage>), Error>;
    /// Resolves to the key address
    async fn state_account_key<V>(
        &self,
        addr: &Address,
        ts: &Arc<Tipset>,
    ) -> Result<Address, Error>
    where
        V: ProofVerifier;
    /// Return all messages for a tipset
    fn messages_for_tipset(&self, h: &Tipset) -> Result<Vec<ChainMessage>, Error>;
    /// Return a tipset given the tipset keys from the ChainStore
    async fn load_tipset(&self, tsk: &TipsetKeys) -> Result<Arc<Tipset>, Error>;
    /// Computes the base fee
    fn chain_compute_base_fee(&self, ts: &Tipset) -> Result<BigInt, Error>;
}

/// This is the default Provider implementation that will be used for the mpool RPC.
pub struct MpoolRpcProvider<DB> {
    subscriber: Publisher<HeadChange>,
    sm: Arc<StateManager<DB>>,
}

impl<DB> MpoolRpcProvider<DB>
where
    DB: BlockStore + Sync + Send,
{
    pub fn new(subscriber: Publisher<HeadChange>, sm: Arc<StateManager<DB>>) -> Self
    where
        DB: BlockStore,
    {
        MpoolRpcProvider { subscriber, sm }
    }
}

#[async_trait]
impl<DB> Provider for MpoolRpcProvider<DB>
where
    DB: BlockStore + Sync + Send + 'static,
{
    async fn subscribe_head_changes(&mut self) -> Subscriber<HeadChange> {
        self.subscriber.subscribe()
    }

    async fn get_heaviest_tipset(&mut self) -> Option<Arc<Tipset>> {
        self.sm.chain_store().heaviest_tipset().await
    }

    fn put_message(&self, msg: &ChainMessage) -> Result<Cid, Error> {
        let cid = self
            .sm
            .blockstore()
            .put(msg, Blake2b256)
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
    ) -> Result<(Vec<UnsignedMessage>, Vec<SignedMessage>), Error> {
        chain::block_messages(self.sm.blockstore(), h).map_err(|err| err.into())
    }

    fn messages_for_tipset(&self, h: &Tipset) -> Result<Vec<ChainMessage>, Error> {
        Ok(self.sm.chain_store().messages_for_tipset(h)?)
    }

    async fn load_tipset(&self, tsk: &TipsetKeys) -> Result<Arc<Tipset>, Error> {
        let ts = self.sm.chain_store().tipset_from_keys(tsk).await?;
        Ok(ts)
    }
    fn chain_compute_base_fee(&self, ts: &Tipset) -> Result<BigInt, Error> {
        chain::compute_base_fee(self.sm.blockstore(), ts).map_err(|err| err.into())
    }
    async fn state_account_key<V>(&self, addr: &Address, ts: &Arc<Tipset>) -> Result<Address, Error>
    where
        V: ProofVerifier,
    {
        self.sm
            .resolve_to_key_addr::<V>(addr, ts)
            .await
            .map_err(|e| Error::Other(e.to_string()))
    }
}
