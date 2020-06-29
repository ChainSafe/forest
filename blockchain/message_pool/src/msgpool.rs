// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::errors::Error;
use address::Address;
use async_std::task;
use blocks::{BlockHeader, Tipset, TipsetKeys};
use blockstore::BlockStore;
use chain::ChainStore;
use cid::multihash::Blake2b256;
use cid::Cid;
use crypto::{Signature, SignatureType};
use encoding::Cbor;
use flo_stream::Subscriber;
use futures::StreamExt;
use log::warn;
use lru::LruCache;
use message::{Message, SignedMessage, UnsignedMessage};
use num_bigint::{BigInt, BigUint};
use state_tree::StateTree;
use std::borrow::BorrowMut;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread::sleep;
use std::time::Duration;
use vm::ActorState;

const REPLACE_BY_FEE_RATIO: f32 = 1.25;
const RBF_NUM: u64 = ((REPLACE_BY_FEE_RATIO - 1f32) * 256f32) as u64;
const RBF_DENOM: u64 = 256;

/// Simple struct that contains a hashmap of messages where k: a message from address, v: a message
/// which corresponds to that address
#[derive(Clone, Default)]
pub struct MsgSet {
    msgs: HashMap<u64, SignedMessage>,
    next_sequence: u64,
}

impl MsgSet {
    /// Generate a new MsgSet with an empty hashmap and a default next_sequence of 0
    pub fn new() -> Self {
        MsgSet {
            msgs: HashMap::new(),
            next_sequence: 0,
        }
    }

    /// Add a signed message to the MsgSet. Increase next_sequence if the message has a sequence greater
    /// than any existing message sequence.
    pub fn add(&mut self, m: &SignedMessage) -> Result<(), Error> {
        if self.msgs.is_empty() || m.sequence() >= self.next_sequence {
            self.next_sequence = m.sequence() + 1;
        }
        if let Some(exms) = self.msgs.get(&m.sequence()) {
            if m.cid()? != exms.cid()? {
                let gas_price = exms.message().gas_price();
                let rbf_num = BigUint::from(RBF_NUM);
                let rbf_denom = BigUint::from(RBF_DENOM);
                let min_price =
                    gas_price.clone() + (gas_price / &rbf_num) + rbf_denom + BigUint::from(1_u64);
                if m.message().gas_price() <= &min_price {
                    // message with duplicate sequence is already in mpool
                    return Err(Error::DuplicateSequence);
                }
            }
        }
        self.msgs.insert(m.sequence(), m.clone());
        Ok(())
    }
}

/// Provider Trait. This trait will be used by the messagepool to interact with some medium in order to do
/// the operations that are listed below that are required for the messagepool.
pub trait Provider {
    /// Update Mpool's cur_tipset whenever there is a chnge to the provider
    fn subscribe_head_changes(&mut self) -> Subscriber<Arc<Tipset>>;
    /// Get the heaviest Tipset in the provider
    fn get_heaviest_tipset(&mut self) -> Option<Tipset>;
    /// Add a message to the MpoolProvider, return either Cid or Error depending on successful put
    fn put_message(&self, msg: &SignedMessage) -> Result<Cid, Error>;
    /// Return state actor for given address given the tipset that the a temp StateTree will be rooted
    /// at. Return ActorState or Error depending on whether or not ActorState is found
    fn state_get_actor(&self, addr: &Address, ts: &Tipset) -> Result<ActorState, Error>;
    /// Return the signed messages for given blockheader
    fn messages_for_block(
        &self,
        h: &BlockHeader,
    ) -> Result<(Vec<UnsignedMessage>, Vec<SignedMessage>), Error>;
    /// Return all messages for a tipset
    fn messages_for_tipset(&self, h: &Tipset) -> Result<Vec<UnsignedMessage>, Error>;
    /// Return a tipset given the tipset keys from the ChainStore
    fn load_tipset(&self, tsk: &TipsetKeys) -> Result<Tipset, Error>;
}

/// This is the mpool provider struct that will let us access and add messages to messagepool.
/// future
pub struct MpoolProvider<DB> {
    cs: ChainStore<DB>,
}

impl<'db, DB> MpoolProvider<DB>
where
    DB: BlockStore,
{
    pub fn new(cs: ChainStore<DB>) -> Self
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
    fn subscribe_head_changes(&mut self) -> Subscriber<Arc<Tipset>> {
        self.cs.subscribe()
    }

    fn get_heaviest_tipset(&mut self) -> Option<Tipset> {
        let ts = self.cs.heaviest_tipset()?;
        Some(ts.as_ref().clone())
    }

    fn put_message(&self, msg: &SignedMessage) -> Result<Cid, Error> {
        let cid = self
            .cs
            .db
            .put(msg, Blake2b256)
            .map_err(|err| Error::Other(err.to_string()))?;
        Ok(cid)
    }

    fn state_get_actor(&self, addr: &Address, ts: &Tipset) -> Result<ActorState, Error> {
        let state = StateTree::new_from_root(self.cs.db.as_ref(), ts.parent_state())
            .map_err(Error::Other)?;
        let actor = state.get_actor(addr).map_err(Error::Other)?;
        actor.ok_or_else(|| Error::Other("No actor state".to_owned()))
    }

    fn messages_for_block(
        &self,
        h: &BlockHeader,
    ) -> Result<(Vec<UnsignedMessage>, Vec<SignedMessage>), Error> {
        chain::block_messages(self.cs.blockstore(), h).map_err(|err| err.into())
    }

    fn messages_for_tipset(&self, h: &Tipset) -> Result<Vec<UnsignedMessage>, Error> {
        self.cs.messages_for_tipset(h).map_err(|err| err.into())
    }

    fn load_tipset(&self, tsk: &TipsetKeys) -> Result<Tipset, Error> {
        self.cs.tipset_from_keys(tsk).map_err(|err| err.into())
    }
}

/// This is the main MessagePool struct
pub struct MessagePool<T: 'static> {
    local_addrs: Vec<Address>,
    pending: HashMap<Address, MsgSet>,
    pub cur_tipset: Mutex<Tipset>,
    api: Arc<Mutex<T>>,
    pub min_gas_price: BigInt,
    pub max_tx_pool_size: i64,
    pub network_name: String,
    bls_sig_cache: LruCache<Cid, Signature>,
    sig_val_cache: LruCache<Cid, ()>,
    local_msgs: HashMap<Vec<u8>, SignedMessage>,
}

impl<T> MessagePool<T>
where
    T: Provider + std::marker::Send,
{
    /// Create a new message pool
    pub fn new(mut api: T, network_name: String) -> Result<Arc<Mutex<MessagePool<T>>>, Error>
    where
        T: Provider,
    {
        // LruCache sizes have been taken from the lotus implementation
        let tipset = Mutex::new(
            api.get_heaviest_tipset()
                .ok_or_else(|| Error::Other("No ts in api to set as cur_tipset".to_owned()))?,
        );
        let bls_sig_cache = LruCache::new(40000);
        let sig_val_cache = LruCache::new(32000);
        let api_mutex = Arc::new(Mutex::new(api));
        let mp = Arc::new(Mutex::new(MessagePool {
            local_addrs: Vec::new(),
            pending: HashMap::new(),
            cur_tipset: tipset,
            api: api_mutex,
            min_gas_price: Default::default(),
            max_tx_pool_size: 5000,
            network_name,
            bls_sig_cache,
            sig_val_cache,
            local_msgs: HashMap::new(),
        }));

        let mut mp_lock = mp.lock().map_err(|_| Error::MutexPoisonError)?;
        mp_lock.load_local()?;
        let mut api = mp_lock.api.lock().map_err(|_| Error::MutexPoisonError)?;
        let mut subscriber = api.subscribe_head_changes();
        drop(api);
        drop(mp_lock);

        let mpool = mp.clone();
        task::spawn(async move {
            loop {
                if let Some(ts) = subscriber.next().await {
                    if let Ok(mut lock) = mpool.lock() {
                        lock.head_change(Vec::new(), vec![ts.as_ref().clone()])
                            .unwrap_or_else(|err| warn!("Error changing head: {:?}", err));
                    }
                }
                sleep(Duration::new(1, 0));
            }
        });
        Ok(mp)
    }

    /// Add a signed message to local_addrs and local_msgs
    fn add_local(&mut self, m: &SignedMessage, msgb: Vec<u8>) -> Result<(), Error> {
        self.local_addrs.push(*m.from());
        self.local_msgs.insert(msgb, m.clone());
        Ok(())
    }

    /// Push a signed message to the MessagePool
    pub fn push(&mut self, msg: &SignedMessage) -> Result<Cid, Error> {
        let msg_serial = msg.marshal_cbor()?;
        self.add(msg)?;
        self.add_local(msg, msg_serial)?;
        msg.cid().map_err(|err| err.into())
    }

    /// This is a helper to push that will help to make sure that the message fits the parameters
    /// to be pushed to the MessagePool
    pub fn add(&mut self, msg: &SignedMessage) -> Result<(), Error> {
        let size = msg.marshal_cbor()?.len();
        if size > 32 * 1024 {
            return Err(Error::MessageTooBig);
        }
        if msg.value() > &BigUint::from(2_000_000_000u64) {
            return Err(Error::MessageValueTooHigh);
        }

        self.verify_msg_sig(msg)?;

        let tmp = msg.clone();
        let tip = self
            .cur_tipset
            .lock()
            .map_err(|_| Error::MutexPoisonError)?
            .clone();
        self.add_tipset(tmp, &tip)
    }

    /// Add a SignedMessage without doing any of the checks
    pub fn add_skip_checks(&mut self, m: SignedMessage) -> Result<(), Error> {
        self.add_helper(m)
    }

    /// Verify the message signature. first check if it has already been verified and put into
    /// cache. If it has not, then manually verify it then put it into cache for future use
    fn verify_msg_sig(&mut self, msg: &SignedMessage) -> Result<(), Error> {
        let cid = msg.cid()?;
        if let Some(()) = self.sig_val_cache.get(&cid) {
            return Ok(());
        }
        let umsg = msg.message().marshal_cbor()?;
        msg.signature()
            .verify(umsg.as_slice(), msg.from())
            .map_err(Error::Other)?;
        self.sig_val_cache.put(cid, ());
        Ok(())
    }

    /// Verify the state_sequence and balance for the sender of the message given then call add_locked
    /// to finish adding the signed_message to pending
    fn add_tipset(&mut self, msg: SignedMessage, cur_ts: &Tipset) -> Result<(), Error> {
        let sequence = self.get_state_sequence(msg.from(), cur_ts)?;

        if sequence > msg.message().sequence() {
            return Err(Error::SequenceTooLow);
        }

        let balance = self.get_state_balance(msg.from(), cur_ts)?;
        let msg_balance = BigInt::from(msg.message().required_funds());
        if balance < msg_balance {
            return Err(Error::NotEnoughFunds);
        }
        self.add_helper(msg)
    }

    /// Finish verifying signed message before adding it to the pending mset hashmap. If an entry
    /// in the hashmap does not yet exist, create a new mset that will correspond to the from message
    /// and push it to the pending hashmap
    fn add_helper(&mut self, msg: SignedMessage) -> Result<(), Error> {
        let api = self
            .api
            .lock()
            .map_err(|err| Error::Other(err.to_string()))?;
        if msg.signature().signature_type() == SignatureType::BLS {
            self.bls_sig_cache.put(msg.cid()?, msg.signature().clone());
        }
        if msg.message().gas_limit() > 100_000_000 {
            return Err(Error::Other(
                "given message has too high of a gas limit".to_string(),
            ));
        }
        api.put_message(&msg)?;

        let msett = self.pending.get_mut(msg.message().from());
        match msett {
            Some(mset) => mset.add(&msg)?,
            None => {
                let mut mset = MsgSet::new();
                mset.add(&msg)?;
                self.pending.insert(*msg.message().from(), mset);
            }
        }
        Ok(())
    }

    /// Get the sequence for a given address, return Error if there is a failure to retrieve sequence
    pub fn get_sequence(&self, addr: &Address) -> Result<u64, Error> {
        let cur_ts = &self
            .cur_tipset
            .lock()
            .map_err(|_| Error::MutexPoisonError)?;
        let sequence = self.get_state_sequence(addr, cur_ts)?;

        match self.pending.get(addr) {
            Some(mset) => {
                if sequence > mset.next_sequence {
                    return Ok(sequence);
                }
                Ok(mset.next_sequence)
            }
            None => Ok(sequence),
        }
    }

    /// Get the state of the base_sequence for a given address in cur_ts
    fn get_state_sequence(&self, addr: &Address, cur_ts: &Tipset) -> Result<u64, Error> {
        let api = self.api.lock().map_err(|_| Error::MutexPoisonError)?;
        let actor = api.state_get_actor(&addr, cur_ts)?;

        let mut base_sequence = actor.sequence;

        // TODO here lotus has a todo, so I guess we should eventually remove cur_ts from one
        // of the params for this method and just use the chain head
        let msgs = api.messages_for_tipset(cur_ts)?;
        for m in msgs {
            if m.from() == addr {
                if m.sequence() != base_sequence {
                    return Err(Error::Other("tipset has bad sequence ordering".to_string()));
                }
                base_sequence += 1;
            }
        }
        Ok(base_sequence)
    }

    /// Get the state balance for the actor that corresponds to the supplied address and tipset,
    /// if this actor does not exist, return an error
    fn get_state_balance(&mut self, addr: &Address, ts: &Tipset) -> Result<BigInt, Error> {
        let api = self.api.lock().map_err(|_| Error::MutexPoisonError)?;
        let actor = api.state_get_actor(&addr, &ts)?;
        Ok(BigInt::from(actor.balance))
    }

    /// Remove a message given a sequence and address from the messagepool
    pub fn remove(&mut self, from: &Address, sequence: u64) -> Result<(), Error> {
        let mset = self
            .pending
            .get_mut(from)
            .ok_or_else(|| Error::InvalidFromAddr)?;
        mset.msgs.remove(&sequence);

        if mset.msgs.is_empty() {
            self.pending.remove(from);
        } else {
            let mut max_sequence: u64 = 0;
            for sequence_temp in mset.msgs.keys().cloned() {
                if max_sequence < sequence_temp {
                    max_sequence = sequence_temp;
                }
            }
            if max_sequence < sequence {
                max_sequence = sequence;
            }
            mset.next_sequence = max_sequence + 1;
        }
        Ok(())
    }

    /// Return a tuple that contains a vector of all signed messages and the current tipset for
    /// self.
    pub fn pending(&self) -> Result<(Vec<SignedMessage>, Tipset), Error> {
        let mut out: Vec<SignedMessage> = Vec::new();
        for (addr, _) in self.pending.clone() {
            out.append(
                self.pending_for(&addr)
                    .ok_or_else(|| Error::InvalidFromAddr)?
                    .as_mut(),
            )
        }
        let cur_ts = self
            .cur_tipset
            .lock()
            .map_err(|_| Error::MutexPoisonError)?
            .clone();
        Ok((out, cur_ts))
    }

    /// Return a Vector of signed messages for a given from address. This vector will be sorted by
    /// each messsage's sequence. If no corresponding messages found, return None result type
    fn pending_for(&self, a: &Address) -> Option<Vec<SignedMessage>> {
        let mset = self.pending.get(a);
        match mset {
            Some(msgset) => {
                if msgset.msgs.is_empty() {
                    return None;
                }

                let mut msg_vec = Vec::new();

                for (_, item) in msgset.msgs.clone() {
                    msg_vec.push(item);
                }

                msg_vec.sort_by_key(|value| value.message().sequence());

                Some(msg_vec)
            }
            None => None,
        }
    }

    /// Return Vector of signed messages given a block header for self
    pub fn messages_for_blocks(
        &mut self,
        blks: &[BlockHeader],
    ) -> Result<Vec<SignedMessage>, Error> {
        let mut msg_vec: Vec<SignedMessage> = Vec::new();
        for block in blks {
            let (umsg, mut smsgs) = self
                .api
                .lock()
                .map_err(|_| Error::MutexPoisonError)?
                .messages_for_block(&block)?;
            msg_vec.append(smsgs.as_mut());
            for msg in umsg {
                let smsg = self.recover_sig(msg)?;
                msg_vec.push(smsg)
            }
        }
        Ok(msg_vec)
    }

    /// Attempt to get the signed message given an unsigned message in message pool
    pub fn recover_sig(&mut self, msg: UnsignedMessage) -> Result<SignedMessage, Error> {
        let val = self
            .bls_sig_cache
            .get(&msg.cid()?)
            .ok_or_else(|| Error::Other("Could not recover sig".to_owned()))?;
        Ok(SignedMessage::new_from_parts(msg, val.clone()))
    }

    /// Return gas price estimate this has been translated from lotus, a more smart implementation will
    /// most likely need to be implemented
    pub fn estimate_gas_price(&self, nblocksincl: u64) -> Result<BigInt, Error> {
        // TODO possibly come up with a smarter way to estimate the gas price
        let min_gas_price = 0;
        match nblocksincl {
            0 => Ok(BigInt::from(min_gas_price + 2)),
            1 => Ok(BigInt::from(min_gas_price + 1)),
            _ => Ok(BigInt::from(min_gas_price)),
        }
    }

    /// Load local messages into pending. As of  right now messages are not deleted from self's
    /// local_message field, possibly implement this in the future?
    pub fn load_local(&mut self) -> Result<(), Error> {
        for (key, value) in self.local_msgs.clone() {
            self.add(&value).unwrap_or_else(|err| {
                if err == Error::SequenceTooLow {
                    warn!("error adding message: {:?}", err);
                    self.local_msgs.remove(&key);
                }
            });
        }
        Ok(())
    }

    /// This is a helper method for head_change. This method will remove a sequence for a from address
    /// from the rmsgs hashmap. Also remove the from address and sequence from the mmessagepool.
    fn rm(
        &mut self,
        from: &Address,
        sequence: u64,
        rmsgs: &mut HashMap<Address, HashMap<u64, SignedMessage>>,
    ) {
        let s = rmsgs.get_mut(from);
        if s.is_none() {
            self.remove(from, sequence).ok();
            return;
        }
        let temp = s.unwrap();
        if temp.get_mut(&sequence).is_some() {
            temp.remove(&sequence);
            return;
        }
        self.remove(from, sequence).ok();
    }

    /// This function will revert and/or apply tipsets to the message pool. This function should be
    /// called every time that there is a head change in the message pool
    pub fn head_change(&mut self, revert: Vec<Tipset>, apply: Vec<Tipset>) -> Result<(), Error> {
        let mut rmsgs: HashMap<Address, HashMap<u64, SignedMessage>> = HashMap::new();
        for ts in revert {
            let pts = self
                .api
                .lock()
                .map_err(|_| Error::MutexPoisonError)?
                .load_tipset(ts.parents())?;
            let msgs = self.messages_for_blocks(ts.blocks())?;
            let parent = pts.clone();
            *self
                .cur_tipset
                .lock()
                .map_err(|_| Error::MutexPoisonError)? = parent;
            for msg in msgs {
                add(msg, rmsgs.borrow_mut());
            }
        }

        for ts in apply {
            for b in ts.blocks() {
                let (msgs, smsgs) = self
                    .api
                    .lock()
                    .map_err(|_| Error::MutexPoisonError)?
                    .messages_for_block(b)?;
                for msg in smsgs {
                    self.rm(msg.from(), msg.sequence(), rmsgs.borrow_mut());
                }

                for msg in msgs {
                    self.rm(msg.from(), msg.sequence(), rmsgs.borrow_mut());
                }
            }
            *self
                .cur_tipset
                .lock()
                .map_err(|_| Error::MutexPoisonError)? = ts;
        }

        for (_, hm) in rmsgs {
            for (_, msg) in hm {
                self.add_skip_checks(msg).ok();
            }
        }
        Ok(())
    }
}

/// This function is a helper method for head_change. This method will add a signed message to the given rmsgs HashMap
fn add(m: SignedMessage, rmsgs: &mut HashMap<Address, HashMap<u64, SignedMessage>>) {
    let s = rmsgs.get_mut(m.from());
    if s.is_none() {
        let mut temp = HashMap::new();
        temp.insert(m.sequence(), m.clone());
        rmsgs.insert(*m.from(), temp);
        return;
    }
    let temp = s.unwrap();
    temp.insert(m.sequence(), m);
}

#[cfg(test)]
mod tests {
    use super::Error as Errors;
    use super::*;
    use crate::MessagePool;
    use address::Address;
    use async_std::task;
    use blocks::{BlockHeader, Ticket, Tipset};
    use cid::Cid;
    use crypto::{election_proof::ElectionProof, SignatureType, VRFProof};
    use flo_stream::{MessagePublisher, Publisher, Subscriber};
    use key_management::{MemKeyStore, Wallet};
    use message::{SignedMessage, UnsignedMessage};
    use num_bigint::BigUint;
    use std::borrow::BorrowMut;
    use std::convert::TryFrom;

    struct TestApi {
        bmsgs: HashMap<Cid, Vec<SignedMessage>>,
        state_sequence: HashMap<Address, u64>,
        tipsets: Vec<Tipset>,
        publisher: Publisher<Arc<Tipset>>,
    }

    impl TestApi {
        pub fn new() -> Self {
            TestApi {
                bmsgs: HashMap::new(),
                state_sequence: HashMap::new(),
                tipsets: Vec::new(),
                publisher: Publisher::new(1),
            }
        }

        pub fn set_state_sequence(&mut self, addr: &Address, sequence: u64) {
            self.state_sequence.insert(addr.clone(), sequence);
        }

        pub fn set_block_messages(&mut self, h: &BlockHeader, msgs: Vec<SignedMessage>) {
            self.bmsgs.insert(h.cid().clone(), msgs.clone());
            self.tipsets.push(Tipset::new(vec![h.clone()]).unwrap())
        }

        pub async fn set_heaviest_tipset(&mut self, ts: Arc<Tipset>) -> () {
            self.publisher.publish(ts).await
        }
    }

    impl Provider for TestApi {
        fn subscribe_head_changes(&mut self) -> Subscriber<Arc<Tipset>> {
            self.publisher.subscribe()
        }

        fn get_heaviest_tipset(&mut self) -> Option<Tipset> {
            Tipset::new(vec![create_header(1, b"", b"")]).ok()
        }

        fn put_message(&self, _msg: &SignedMessage) -> Result<Cid, Errors> {
            Ok(Cid::default())
        }

        fn state_get_actor(&self, addr: &Address, _ts: &Tipset) -> Result<ActorState, Errors> {
            let s = self.state_sequence.get(addr);
            let mut sequence = 0;
            if s.is_some() {
                sequence = s.unwrap().clone();
            }
            let actor = ActorState::new(
                Cid::default(),
                Cid::default(),
                BigUint::from(9_000_000 as u64),
                sequence,
            );
            Ok(actor)
        }

        fn messages_for_block(
            &self,
            h: &BlockHeader,
        ) -> Result<(Vec<UnsignedMessage>, Vec<SignedMessage>), Errors> {
            let v: Vec<UnsignedMessage> = Vec::new();
            let thing = self.bmsgs.get(h.cid());
            match thing {
                Some(s) => Ok((v, s.clone())),
                None => {
                    let temp: Vec<SignedMessage> = Vec::new();
                    Ok((v, temp))
                }
            }
        }

        fn messages_for_tipset(&self, h: &Tipset) -> Result<Vec<UnsignedMessage>, Errors> {
            let (us, s) = self.messages_for_block(&h.blocks()[0]).unwrap();
            let mut msgs = Vec::new();
            for msg in us {
                msgs.push(msg);
            }
            for smsg in s {
                msgs.push(smsg.message().clone());
            }
            Ok(msgs)
        }

        fn load_tipset(&self, tsk: &TipsetKeys) -> Result<Tipset, Errors> {
            for ts in &self.tipsets {
                if tsk.cids == ts.cids() {
                    return Ok(ts.clone());
                }
            }
            Err(Errors::InvalidToAddr)
        }
    }

    fn create_header(weight: u64, parent_bz: &[u8], cached_bytes: &[u8]) -> BlockHeader {
        let header = BlockHeader::builder()
            .weight(BigUint::from(weight))
            .cached_bytes(cached_bytes.to_vec())
            .cached_cid(Cid::new_from_cbor(parent_bz, Blake2b256))
            .miner_address(Address::new_id(0))
            .build()
            .unwrap();
        header
    }

    fn create_smsg(
        to: &Address,
        from: &Address,
        wallet: &mut Wallet<MemKeyStore>,
        sequence: u64,
    ) -> SignedMessage {
        let umsg: UnsignedMessage = UnsignedMessage::builder()
            .to(to.clone())
            .from(from.clone())
            .sequence(sequence)
            .build()
            .unwrap();
        let message_cbor = Cbor::marshal_cbor(&umsg).unwrap();
        let sig = wallet.sign(&from, message_cbor.as_slice()).unwrap();
        SignedMessage::new_from_parts(umsg, sig)
    }

    fn mock_block(weight: u64, ticket_sequence: u64) -> BlockHeader {
        let addr = Address::new_id(1234561);
        let c =
            Cid::try_from("bafyreicmaj5hhoy5mgqvamfhgexxyergw7hdeshizghodwkjg6qmpoco7i").unwrap();

        let fmt_str = format!("===={}=====", ticket_sequence);
        let ticket = Ticket::new(VRFProof::new(fmt_str.clone().into_bytes()));
        let election_proof = ElectionProof {
            vrfproof: VRFProof::new(fmt_str.into_bytes()),
        };
        let weight_inc = BigUint::from(weight);
        BlockHeader::builder()
            .miner_address(addr)
            .election_proof(Some(election_proof))
            .ticket(ticket)
            .message_receipts(c.clone())
            .messages(c.clone())
            .state_root(c)
            .weight(weight_inc)
            .build_and_validate()
            .unwrap()
    }

    fn mock_block_with_parents(parents: Tipset, weight: u64, ticket_sequence: u64) -> BlockHeader {
        let addr = Address::new_id(1234561);
        let c =
            Cid::try_from("bafyreicmaj5hhoy5mgqvamfhgexxyergw7hdeshizghodwkjg6qmpoco7i").unwrap();

        let height = parents.epoch() + 1;

        let mut weight_inc = BigUint::from(weight);
        weight_inc = parents.blocks()[0].weight() + weight_inc;
        let fmt_str = format!("===={}=====", ticket_sequence);
        let ticket = Ticket::new(VRFProof::new(fmt_str.clone().into_bytes()));
        let election_proof = ElectionProof {
            vrfproof: VRFProof::new(fmt_str.into_bytes()),
        };
        BlockHeader::builder()
            .miner_address(addr)
            .election_proof(Some(election_proof))
            .ticket(ticket)
            .parents(parents.key().clone())
            .message_receipts(c.clone())
            .messages(c.clone())
            .state_root(c)
            .weight(weight_inc)
            .epoch(height)
            .build_and_validate()
            .unwrap()
    }

    #[test]
    fn test_message_pool() {
        let keystore = MemKeyStore::new();
        let mut wallet = Wallet::new(keystore);
        let sender = wallet.generate_addr(SignatureType::Secp256k1).unwrap();
        let target = wallet.generate_addr(SignatureType::Secp256k1).unwrap();

        let mut tma = TestApi::new();
        tma.set_state_sequence(&sender, 0);

        let mpool = MessagePool::new(tma, "mptest".to_string()).unwrap();
        let mut mpool_locked = mpool.lock().unwrap();

        let mut smsg_vec = Vec::new();
        for i in 0..4 {
            let msg = create_smsg(&target, &sender, wallet.borrow_mut(), i);
            smsg_vec.push(msg);
        }

        assert_eq!(mpool_locked.get_sequence(&sender).unwrap(), 0);
        mpool_locked.push(&smsg_vec[0]).unwrap();
        assert_eq!(mpool_locked.get_sequence(&sender).unwrap(), 1);
        mpool_locked.push(&smsg_vec[1]).unwrap();
        assert_eq!(mpool_locked.get_sequence(&sender).unwrap(), 2);
        mpool_locked.push(&smsg_vec[2]).unwrap();
        assert_eq!(mpool_locked.get_sequence(&sender).unwrap(), 3);
        mpool_locked.push(&smsg_vec[3]).unwrap();
        assert_eq!(mpool_locked.get_sequence(&sender).unwrap(), 4);

        let a = mock_block(1, 1);

        mpool_locked
            .api
            .lock()
            .unwrap()
            .set_block_messages(&a, smsg_vec);
        mpool_locked
            .head_change(Vec::new(), vec![Tipset::new(vec![a]).unwrap()])
            .unwrap();

        assert_eq!(mpool_locked.get_sequence(&sender).unwrap(), 4);

        drop(mpool_locked);
        assert_eq!(mpool.lock().unwrap().get_sequence(&sender).unwrap(), 4);
    }

    #[test]
    fn test_revert_messages() {
        let tma = TestApi::new();
        let mut wallet = Wallet::new(MemKeyStore::new());
        let mpool = MessagePool::new(tma, "mptest".to_string()).unwrap();
        let mut mpool_locked = mpool.lock().unwrap();

        let a = mock_block(1, 1);
        let tipset = Tipset::new(vec![a.clone()]).unwrap();
        let b = mock_block_with_parents(tipset, 1, 1);

        let sender = wallet.generate_addr(SignatureType::BLS).unwrap();
        let target = Address::new_id(1001);

        let mut smsg_vec = Vec::new();

        for i in 0..4 {
            let msg = create_smsg(&target, &sender, wallet.borrow_mut(), i);
            smsg_vec.push(msg);
        }

        mpool_locked
            .api
            .lock()
            .unwrap()
            .set_block_messages(&a, vec![smsg_vec[0].clone()]);
        mpool_locked
            .api
            .lock()
            .unwrap()
            .set_block_messages(&b.clone(), smsg_vec[1..4].to_vec());

        mpool_locked
            .api
            .lock()
            .unwrap()
            .set_state_sequence(&sender, 0);

        mpool_locked.add(&smsg_vec[0]).unwrap();
        mpool_locked.add(&smsg_vec[1]).unwrap();
        mpool_locked.add(&smsg_vec[2]).unwrap();
        mpool_locked.add(&smsg_vec[3]).unwrap();

        mpool_locked
            .api
            .lock()
            .unwrap()
            .set_state_sequence(&sender, 0);
        mpool_locked
            .head_change(Vec::new(), vec![Tipset::new(vec![a]).unwrap()])
            .unwrap();
        assert_eq!(mpool_locked.get_sequence(&sender).unwrap(), 4);

        mpool_locked
            .api
            .lock()
            .unwrap()
            .set_state_sequence(&sender, 1);
        mpool_locked
            .head_change(Vec::new(), vec![Tipset::new(vec![b.clone()]).unwrap()])
            .unwrap();
        assert_eq!(mpool_locked.get_sequence(&sender).unwrap(), 4);

        mpool_locked
            .api
            .lock()
            .unwrap()
            .set_state_sequence(&sender, 0);
        mpool_locked
            .head_change(vec![Tipset::new(vec![b]).unwrap()], Vec::new())
            .unwrap();
        assert_eq!(mpool_locked.get_sequence(&sender).unwrap(), 4);

        let (p, _) = mpool_locked.pending().unwrap();
        assert_eq!(p.len(), 3);
    }

    #[test]
    fn test_async_message_pool() {
        let keystore = MemKeyStore::new();
        let mut wallet = Wallet::new(keystore);
        let sender = wallet.generate_addr(SignatureType::Secp256k1).unwrap();
        let target = wallet.generate_addr(SignatureType::Secp256k1).unwrap();

        let mut tma = TestApi::new();
        tma.set_state_sequence(&sender, 0);

        let mpool = MessagePool::new(tma, "mptest".to_string()).unwrap();
        let mut mpool_locked = mpool.lock().unwrap();

        let mut smsg_vec = Vec::new();
        for i in 0..3 {
            let msg = create_smsg(&target, &sender, wallet.borrow_mut(), i);
            smsg_vec.push(msg);
        }

        assert_eq!(mpool_locked.get_sequence(&sender).unwrap(), 0);
        mpool_locked.push(&smsg_vec[0]).unwrap();
        assert_eq!(mpool_locked.get_sequence(&sender).unwrap(), 1);
        mpool_locked.push(&smsg_vec[1]).unwrap();
        assert_eq!(mpool_locked.get_sequence(&sender).unwrap(), 2);
        mpool_locked.push(&smsg_vec[2]).unwrap();
        assert_eq!(mpool_locked.get_sequence(&sender).unwrap(), 3);

        drop(mpool_locked);

        let header = mock_block(1, 1);
        let tipset = Tipset::new(vec![header.clone()]).unwrap();

        let updater = mpool.lock().unwrap();
        let temp = updater.api.clone();
        let mut api = temp.lock().unwrap();
        drop(updater);

        let ts = tipset.clone();
        task::block_on(async move {
            // updater.api.lock().unwrap().set_block_messages(&header, vec![message]);
            api.set_heaviest_tipset(Arc::new(ts)).await;
        });

        // sleep allows for async block to update mpool's cur_tipset
        sleep(Duration::new(2, 0));

        let locked = mpool.lock().unwrap();
        let cur_ts = locked.cur_tipset.lock().unwrap().clone();
        assert_eq!(cur_ts, tipset);
    }
}
