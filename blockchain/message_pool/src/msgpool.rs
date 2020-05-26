use super::errors::Error;
use crate::errors::Error::DuplicateNonce;
use address::Address;
use blocks::{BlockHeader, Tipset, TipsetKeys};
use blockstore::BlockStore;
use chain::ChainStore;
use cid::multihash::Blake2b256;
use cid::Cid;
use crypto::Signature;
use interpreter::{resolve_to_key_addr, DefaultSyscalls, VM};
use lru::LruCache;
use message::{Message, SignedMessage, UnsignedMessage};
use num_bigint::{BigInt, ToBigInt};
use state_manager::StateManager;
use state_tree::StateTree;
use std::collections::HashMap;
use vm::ActorState;

struct MsgSet {
    msgs: HashMap<u64, SignedMessage>,
    next_nonce: u64,
}

impl MsgSet {
    pub fn new() -> Self {
        MsgSet {
            msgs: HashMap::new(),
            next_nonce: 0,
        }
    }

    pub fn add(&mut self, m: SignedMessage) -> Result<(), Error> {
        if self.msgs.is_empty() || m.sequence() >= self.next_nonce {
            self.next_nonce = m.sequence() + 1;
        }
        if self.msgs.contains_key(&m.sequence()) {
            // need to fix in the event that there's an err raised from calling this next line
            return Err(DuplicateNonce);
        }
        self.msgs.insert(m.sequence(), m);
        return Ok(());
    }
}

trait Provider {
    fn put_message(&self, msg: &SignedMessage) -> Result<Cid, Error>;
    fn state_get_actor(&self, addr: &Address) -> Result<Option<ActorState>, Error>;
    fn state_account_key(&self, addr: &Address, ts: Tipset) -> Result<Address, Error>; // TODO dunno how to do this
    fn messages_for_block(
        &self,
        h: &BlockHeader,
    ) -> Result<(Vec<UnsignedMessage>, Vec<SignedMessage>), Error>;
    fn messages_for_tipset(&self, h: &Tipset) -> Result<Vec<SignedMessage>, Error>;
    fn load_tipset(&self, tsk: &TipsetKeys) -> Result<Tipset, Error>; // TODO dunno how to do this
}

struct MpoolProvider<DB> {
    sm: StateManager<DB>,
}

impl<'db, DB> MpoolProvider<DB>
where
    DB: BlockStore,
{
    pub fn new(sm: StateManager<DB>) -> Self
    where
        DB: BlockStore,
    {
        MpoolProvider { sm }
    }
}

impl<DB> Provider for MpoolProvider<DB>
where
    DB: BlockStore,
{
    fn put_message(&self, msg: &SignedMessage) -> Result<Cid, Error> {
        let cid = self
            .sm
            .get_cs()
            .db
            .put(msg, Blake2b256)
            .map_err(|err| Error::Other(err.to_string()))?;
        return Ok(cid);
    }

    fn state_get_actor(&self, addr: &Address) -> Result<Option<ActorState>, Error> {
        let state = StateTree::new(self.sm.get_cs().db.as_ref());
        //TODO need to have this error be an Error::Other from state_manager errs
        state.get_actor(addr).map_err(|err| Error::Other(err))
    }

    fn state_account_key(&self, addr: &Address, ts: Tipset) -> Result<Address, Error> {
        unimplemented!()
    }

    fn messages_for_block(
        &self,
        h: &BlockHeader,
    ) -> Result<(Vec<UnsignedMessage>, Vec<SignedMessage>), Error> {
        self.sm
            .get_cs()
            .messages(h)
            .map_err(|err| Error::Other(err.to_string()))
    }

    fn messages_for_tipset(&self, h: &Tipset) -> Result<Vec<SignedMessage>, Error> {
        unimplemented!()
    }

    fn load_tipset(&self, tsk: &TipsetKeys) -> Result<Tipset, Error> {
        self.sm
            .get_cs()
            .tipset_from_keys(tsk)
            .map_err(|err| Error::Other(err.to_string()))
    }
}

struct MessagePool<DB> {
    // need to inquire about closer in golang and rust equivalent
    local_addrs: HashMap<String, String>,
    pending: HashMap<String, MsgSet>,
    cur_tipset: String,     // need to wait until pubsub is done
    api: MpoolProvider<DB>, // will need to replace with provider type
    min_gas_price: BigInt,
    max_tx_pool_size: i64,
    network_name: String,
    bls_sig_cache: LruCache<Cid, Signature>,
    sig_val_cache: LruCache<String, ()>,
}

impl<DB> MessagePool<DB>
where
    DB: BlockStore,
{
    pub fn new(api: MpoolProvider<DB>, network_name: String) -> Self
    where
        DB: BlockStore,
    {
        // LruCache sizes have been taken from the lotus implementation
        let bls_sig_cache = LruCache::new(40000);
        let sig_val_cache = LruCache::new(32000);
        MessagePool {
            local_addrs: HashMap::new(),
            pending: HashMap::new(),
            cur_tipset: "tmp".to_string(), // cannnot do this yet, need pubsub done
            api,
            min_gas_price: ToBigInt::to_bigint(&0).unwrap(),
            max_tx_pool_size: 5000,
            network_name,
            bls_sig_cache,
            sig_val_cache,
        }
    }
}
