
use tokio::sync::broadcast::{error::RecvError, Receiver as Subscriber, Sender as Publisher};
use chain::{HeadChange, MINIMUM_BASE_FEE};
use async_std::sync::{Arc, RwLock};
use blockstore::BlockStore;
use async_trait::async_trait;
use state_manager::StateManager;
use crate::Provider;
use message::{ChainMessage, Message, SignedMessage, UnsignedMessage};
use cid::Cid;
use blocks::Tipset;
use crate::errors::Error;
use cid::Code::Blake2b256;
use address::Address;
use vm::ActorState;
use state_tree::StateTree;
use blocks::BlockHeader;
use blocks::TipsetKeys;
use num_bigint::BigInt;
use types::verifier::ProofVerifier;

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
