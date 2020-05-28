// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::errors::Error;
use crate::errors::Error::DuplicateNonce;
use address::Address;
use blocks::{BlockHeader, Tipset, TipsetKeys};
use blockstore::BlockStore;
use chain::ChainStore;
use cid::multihash::Blake2b256;
use cid::Cid;
use crypto::{Signature, SignatureType};
use encoding::Cbor;
use lru::LruCache;
use message::{Message, SignedMessage, UnsignedMessage};
use num_bigint::{BigInt, BigUint, ToBigInt, ToBigUint};
use state_tree::StateTree;
use std::{collections::HashMap, str::from_utf8};
use vm::ActorState;

/// Simple struct that contains a hashmap of messages where k: a message from address, v: a message
/// which corresponds to that address
struct MsgSet {
    msgs: HashMap<u64, SignedMessage>,
    next_nonce: u64,
}

impl MsgSet {
    /// Generate a new MsgSet with an empty hashmap and a default next_nonce of 0
    pub fn new() -> Self {
        MsgSet {
            msgs: HashMap::new(),
            next_nonce: 0,
        }
    }

    /// Add a signed message to the MsgSet. Increase next_nonce if the message has a nonce greater
    /// than any existing message nonce.
    pub fn add(&mut self, m: &SignedMessage) -> Result<(), Error> {
        if self.msgs.is_empty() || m.sequence() >= self.next_nonce {
            self.next_nonce = m.sequence() + 1;
        }
        if self.msgs.contains_key(&m.sequence()) {
            // need to fix in the event that there's an err raised from calling this next line
            let exms = self.msgs.get(&m.sequence()).unwrap();
            if m.cid().map_err(|err| Error::Other(err.to_string()))?
                != exms.cid().map_err(|err| Error::Other(err.to_string()))?
            {
                let gas_price = exms.message().gas_price();
                let replace_by_fee_ratio: f32 = 1.25;
                let rbf_num =
                    BigUint::from(((replace_by_fee_ratio - 1 as f32) * 256 as f32) as u64);
                let rbf_denom = BigUint::from(256 as u64);
                let min_price = gas_price.clone() + (gas_price / &rbf_num) + rbf_denom;
                if m.message().gas_price() <= &min_price {
                    // message with duplicate nonce is already in mpool
                    return Err(DuplicateNonce);
                }
            }
        }
        self.msgs.insert(m.sequence(), m.clone());
        Ok(())
    }
}

trait Provider {
    fn put_message(&self, msg: &SignedMessage) -> Result<Cid, Error>;
    fn state_get_actor(&self, addr: &Address, ts: &Tipset) -> Result<ActorState, Error>;
    fn state_account_key(&self, addr: &Address, ts: Tipset) -> Result<Address, Error>; // TODO dunno how to do this
    fn messages_for_block(
        &self,
        h: &BlockHeader,
    ) -> Result<(Vec<UnsignedMessage>, Vec<SignedMessage>), Error>;
    fn messages_for_tipset(&self, h: &Tipset) -> Result<Vec<UnsignedMessage>, Error>;
    fn load_tipset(&self, tsk: &TipsetKeys) -> Result<Tipset, Error>; // TODO dunno how to do this
}

/// This is the mpool provider struct that will let us access and add messages to messagepool.
/// future TODO is to add a pubsub field to allow for publishing updates. Future TODO is also to
/// add a subscribe_head_change function in order to actually get a functioning messagepool
struct MpoolProvider<DB> {
    cs: ChainStore<DB>,
}

impl<'db, DB> MpoolProvider<DB>
where
    DB: BlockStore,
{
    fn new(cs: ChainStore<DB>) -> Self
    where
        DB: BlockStore,
    {
        MpoolProvider { cs }
    }
}

impl<DB> Provider for MpoolProvider<DB>
where
    DB: BlockStore,
{
    /// Add a message to the MpoolProvider, return either Cid or Error depending on successful put
    fn put_message(&self, msg: &SignedMessage) -> Result<Cid, Error> {
        let cid = self
            .cs
            .db
            .put(msg, Blake2b256)
            .map_err(|err| Error::Other(err.to_string()))?;
        Ok(cid)
    }

    /// Return state actor for given address given the tipset that the a temp StateTree will be rooted
    /// at. Return ActorState or Error depending on whether or not ActorState is found
    fn state_get_actor(&self, addr: &Address, ts: &Tipset) -> Result<ActorState, Error> {
        let state = StateTree::new_from_root(self.cs.db.as_ref(), ts.parent_state())
            .map_err(|err| Error::Other(err))?;
        //TODO need to have this error be an Error::Other from state_manager errs
        let actor = state.get_actor(addr).map_err(Error::Other)?;
        match actor {
            Some(actor_state) => Ok(actor_state),
            None => Err(Error::Other("No actor state".to_string())),
        }
    }

    /// TODO implement this method when we can resolve to key address given a temp StateManager
    fn state_account_key(&self, addr: &Address, ts: Tipset) -> Result<Address, Error> {
        unimplemented!()
    }

    /// Return the signed messages for given blockheader
    fn messages_for_block(
        &self,
        h: &BlockHeader,
    ) -> Result<(Vec<UnsignedMessage>, Vec<SignedMessage>), Error> {
        self.cs
            .messages(h)
            .map_err(|err| Error::Other(err.to_string()))
    }

    /// Return all messages for a tipset
    fn messages_for_tipset(&self, h: &Tipset) -> Result<Vec<UnsignedMessage>, Error> {
        let mut umsg: Vec<UnsignedMessage> = Vec::new();
        let mut msg: Vec<SignedMessage> = Vec::new();
        for bh in h.blocks().iter() {
            let (mut bh_umsg_tmp, mut bh_msg_tmp) = self.messages_for_block(bh)?;
            let bh_umsg = bh_umsg_tmp.as_mut();
            let bh_msg = bh_msg_tmp.as_mut();
            umsg.append(bh_umsg);
            msg.append(bh_msg);
        }
        for msg in &msg {
            umsg.push(msg.message().clone());
        }
        Ok(umsg)
        // unimplemented!()
    }

    /// Return a tipset given the tipset keys from the ChainStore
    fn load_tipset(&self, tsk: &TipsetKeys) -> Result<Tipset, Error> {
        self.cs
            .tipset_from_keys(tsk)
            .map_err(|err| Error::Other(err.to_string()))
    }
}

/// This is the main MessagePool struct TODO async safety as well as get a tipset for the cur_tipset
/// field. This can only be done when subscribe to new heads has been completed
struct MessagePool<DB> {
    // need to inquire about closer in golang and rust equivalent
    local_addrs: HashMap<String, String>,
    pending: HashMap<Address, MsgSet>,
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
    /// Create a new MessagePool. This is not yet functioning as per the outlined TODO above
    pub fn new(api: MpoolProvider<DB>, network_name: String) -> Self
    where
        DB: BlockStore,
    {
        // TODO create tipset
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

    /// Push a signed message to the MessagePool
    pub fn push(&mut self, msg: &SignedMessage) -> Result<Cid, Error> {
        // TODO will be used to addlocal which still needs to be implemented
        let msg_serial = msg
            .marshal_cbor()
            .map_err(|err| return Error::Other(err.to_string()))?;
        self.add(msg)?;
        // TODO do pubsub publish with mp.netName and msg_serial
        msg.cid().map_err(|err| Error::Other(err.to_string()))
    }

    /// This is a helper to push that will help to make sure that the message fits the parameters
    /// to be pushed to the MessagePool
    fn add(&mut self, msg: &SignedMessage) -> Result<(), Error> {
        let size = msg
            .marshal_cbor()
            .map_err(|err| return Error::Other(err.to_string()))?
            .len();
        if size > 32 * 1024 {
            return Err(Error::MessageTooBig);
        }
        if msg
            .value()
            .gt(&ToBigUint::to_biguint(&2_000_000_000).unwrap())
        {
            return Err(Error::MessageValueTooHigh);
        }

        self.verify_msg_sig(msg)?;

        // TODO uncomment this when cur tipset is implemented
        // self.add_tipset(msg, self.cur_tipset)?;
        Ok(())
    }

    /// Return the string representation of the message signature
    fn sig_cache_key(&mut self, msg: &SignedMessage) -> Result<String, Error> {
        match msg.signature().signature_type() {
            SignatureType::Secp256 => Ok(msg.cid().unwrap().to_string()),
            SignatureType::BLS => {
                if msg.signature().bytes().len() < 90 {
                    return Err(Error::BLSSigTooShort);
                }
                let slice = from_utf8(&msg.signature().bytes()[64..]).unwrap();
                let mut beginning = from_utf8(&msg.cid().unwrap().to_bytes())
                    .unwrap()
                    .to_string();
                beginning.push_str(slice);
                Ok(beginning)
            }
        }
    }

    /// Verify the message signature. first check if it has already been verified and put into
    /// cache. If it has not, then manually verify it then put it into cache for future use
    fn verify_msg_sig(&mut self, msg: &SignedMessage) -> Result<(), Error> {
        let sck = self.sig_cache_key(msg)?;
        let is_verif = self.sig_val_cache.get(&sck);
        match is_verif {
            Some(()) => return Ok(()),
            None => {
                let verif = msg
                    .signature()
                    .verify(&msg.message().cid().unwrap().to_bytes(), msg.from());
                match verif {
                    Ok(()) => {
                        self.sig_val_cache.put(sck, ());
                        Ok(())
                    }
                    Err(value) => Err(Error::Other(value)),
                }
            }
        }
    }

    /// Verify the state_nonce and balance for the sender of the message given then call add_locked
    /// to finish adding the signed_message to pending
    fn add_tipset(&mut self, msg: &SignedMessage, cur_ts: &Tipset) -> Result<(), Error> {
        let snonce = self.get_state_nonce(msg.from(), cur_ts)?;

        if snonce > msg.message().sequence() {
            return Err(Error::NonceTooLow);
        }

        let balance = self.get_state_balance(msg.from(), cur_ts)?;
        let msg_balance = BigInt::from(msg.message().required_funds());
        if balance.lt(&msg_balance) {
            return Err(Error::NotEnoughFunds);
        }
        self.add_locked(msg)
    }

    /// Finish verifying signed message before adding it to the pending mset hashmap. If an entry
    /// in the hashmap does not yet exist, create a new mset that will correspond to the from message
    /// and push it to the pending hashmap
    fn add_locked(&mut self, msg: &SignedMessage) -> Result<(), Error> {
        if msg.signature().signature_type() == SignatureType::BLS {
            self.bls_sig_cache.put(
                msg.cid().map_err(|err| Error::Other(err.to_string()))?,
                msg.signature().clone(),
            );
        }
        if msg.message().gas_limit() > 100_000_000 {
            return Err(Error::Other(
                "given message has too high of a gas limit".to_string(),
            ));
        }
        self.api.put_message(msg)?;

        let msett = self.pending.get_mut(msg.message().from());
        match msett {
            Some(mset) => mset.add(msg).map_err(|err| Error::Other(err.to_string()))?,
            None => {
                let mut mset = MsgSet::new();
                mset.add(msg).map_err(|err| Error::Other(err.to_string()))?;
                self.pending.insert(msg.message().from().clone(), mset);
            }
        }
        // TODO pubsub msg
        Ok(())
    }

    /// Get the state of the base_nonce for a given address in cur_ts
    fn get_state_nonce(&self, addr: &Address, cur_ts: &Tipset) -> Result<u64, Error> {
        let actor = self.api.state_get_actor(&addr, cur_ts)?;

        let base_nonce = actor.sequence;

        // TODO will need to chang e this to set cur_ts to chain.head
        // will implement this once we have subscribe to head change done
        // let msgs = self.api.messages_for_tipset(cur_ts).unwrap();

        // TODO will need to call messages_for_tipset after it is implemented
        // and iterate over the messages, and check whether or not the from
        // addr from each message equals addr, if it is not throw error, otherwise
        // increase base_nonce by 1 and then after loop termpinates return base_nonce

        Ok(base_nonce)
    }

    /// Get the state balance for the actor that corresponds to the supplied address and tipset,
    /// if this actor does not exist, return an error
    fn get_state_balance(&self, addr: &Address, ts: &Tipset) -> Result<BigInt, Error> {
        let actor = self.api.state_get_actor(&addr, &ts)?;
        return Ok(BigInt::from(actor.balance));
    }
}
