// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::config::MpoolConfig;
use super::errors::Error;
use address::{Address, Protocol};
use async_std::sync::{Arc, RwLock};
use async_std::task;
use async_trait::async_trait;
use blocks::{BlockHeader, Tipset, TipsetKeys};
use blockstore::BlockStore;
use chain::{HeadChange, MINIMUM_BASE_FEE};
use cid::multihash::Blake2b256;
use cid::Cid;
use crypto::{Signature, SignatureType};
use db::Store;
use encoding::Cbor;
use flo_stream::Subscriber;
use futures::StreamExt;
use log::{error, warn};
use lru::LruCache;
use message::{Message, SignedMessage, UnsignedMessage};
use num_bigint::BigInt;
use state_manager::StateManager;
use state_tree::StateTree;
use std::borrow::BorrowMut;
use std::collections::{HashMap, HashSet};
use vm::ActorState;

const REPLACE_BY_FEE_RATIO: f32 = 1.25;
const RBF_NUM: u64 = ((REPLACE_BY_FEE_RATIO - 1f32) * 256f32) as u64;
const RBF_DENOM: u64 = 256;
const BASE_FEE_LOWER_BOUND_FACTOR_CONSERVATIVE: i64 = 100;

/// Simple struct that contains a hashmap of messages where k: a message from address, v: a message
/// which corresponds to that address
#[derive(Clone, Default, Debug)]
pub struct MsgSet {
    msgs: HashMap<u64, SignedMessage>,
    next_sequence: u64,
    required_funds: BigInt,
}

impl MsgSet {
    /// Generate a new MsgSet with an empty hashmap and a default next_sequence of 0
    pub fn new(sequence: u64) -> Self {
        MsgSet {
            msgs: HashMap::new(),
            next_sequence: sequence,
            required_funds: Default::default(),
        }
    }

    /// Add a signed message to the MsgSet. Increase next_sequence if the message has a sequence greater
    /// than any existing message sequence.
    pub fn add(&mut self, m: SignedMessage) -> Result<(), Error> {
        if self.msgs.is_empty() || m.sequence() >= self.next_sequence {
            self.next_sequence = m.sequence() + 1;
        }
        if let Some(exms) = self.msgs.get(&m.sequence()) {
            if m.cid()? != exms.cid()? {
                let premium = exms.message().gas_premium();
                let rbf_num = BigInt::from(RBF_NUM);
                let rbf_denom = BigInt::from(RBF_DENOM);
                let min_price = premium + ((premium * &rbf_num) / rbf_denom) + 1u8;
                if m.message().gas_premium() <= &min_price {
                    warn!("message gas price is below min gas price");
                    return Err(Error::GasPriceTooLow);
                }
            } else {
                warn!("try to add message with duplicate sequence increase gas premium");
                return Err(Error::DuplicateSequence);
            }
        }
        self.msgs.insert(m.sequence(), m);
        Ok(())
    }
    pub fn rm(&mut self, sequence: u64, applied: bool) {
        let m = if let Some(m) = self.msgs.remove(&sequence) {
            m
        } else {
            if applied && sequence >= self.next_sequence {
                self.next_sequence = sequence + 1;
                while self.msgs.get(&self.next_sequence).is_some() {
                    self.next_sequence += 1;
                }
            }
            return;
        };
        self.required_funds -= m.required_funds();

        // adjust next sequence
        if applied {
            // we removed a (known) message because it was applied in a tipset
            // we can't possibly have filled a gap in this case
            if sequence >= self.next_sequence {
                self.next_sequence = sequence + 1;
            }
            return;
        }
        // we removed a message because it was pruned
        // we have to adjust the sequence if it creates a gap or rewinds state
        if sequence < self.next_sequence {
            self.next_sequence = sequence;
        }
    }

    fn get_required_funds(&self, sequence: u64) -> BigInt {
        let required_funds = self.required_funds.clone();
        match self.msgs.get(&sequence) {
            Some(m) => required_funds - m.required_funds(),
            None => required_funds,
        }
    }
}

/// Provider Trait. This trait will be used by the messagepool to interact with some medium in order to do
/// the operations that are listed below that are required for the messagepool.
#[async_trait]
pub trait Provider {
    /// Update Mpool's cur_tipset whenever there is a chnge to the provider
    async fn subscribe_head_changes(&mut self) -> Subscriber<HeadChange>;
    /// Get the heaviest Tipset in the provider
    async fn get_heaviest_tipset(&mut self) -> Option<Tipset>;
    /// Add a message to the MpoolProvider, return either Cid or Error depending on successful put
    fn put_message(&self, msg: &SignedMessage) -> Result<Cid, Error>;
    /// Return state actor for given address given the tipset that the a temp StateTree will be rooted
    /// at. Return ActorState or Error depending on whether or not ActorState is found
    fn get_actor_after(&self, addr: &Address, ts: &Tipset) -> Result<ActorState, Error>;
    /// Return the signed messages for given blockheader
    fn messages_for_block(
        &self,
        h: &BlockHeader,
    ) -> Result<(Vec<UnsignedMessage>, Vec<SignedMessage>), Error>;
    /// Resolves to the key address
    async fn state_account_key(&self, addr: &Address, ts: &Tipset) -> Result<Address, Error>;
    /// Return all messages for a tipset
    fn messages_for_tipset(&self, h: &Tipset) -> Result<Vec<UnsignedMessage>, Error>;
    /// Return a tipset given the tipset keys from the ChainStore
    fn load_tipset(&self, tsk: &TipsetKeys) -> Result<Tipset, Error>;
    /// Computes the base fee
    fn chain_compute_base_fee(&self, ts: &Tipset) -> Result<BigInt, Error>;
}

/// This is the Provider implementation that will be used for the mpool RPC
pub struct MpoolRpcProvider<DB> {
    subscriber: Subscriber<HeadChange>,
    sm: Arc<StateManager<DB>>,
}

impl<DB> MpoolRpcProvider<DB>
where
    DB: BlockStore + Sync + Send,
{
    pub fn new(subscriber: Subscriber<HeadChange>, sm: Arc<StateManager<DB>>) -> Self
    where
        DB: BlockStore,
    {
        MpoolRpcProvider { subscriber, sm }
    }
}

#[async_trait]
impl<DB> Provider for MpoolRpcProvider<DB>
where
    DB: BlockStore + Sync + Send,
{
    async fn subscribe_head_changes(&mut self) -> Subscriber<HeadChange> {
        self.subscriber.clone()
    }

    async fn get_heaviest_tipset(&mut self) -> Option<Tipset> {
        chain::get_heaviest_tipset(self.sm.blockstore())
            .ok()
            .unwrap_or(None)
    }

    fn put_message(&self, msg: &SignedMessage) -> Result<Cid, Error> {
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

    fn messages_for_tipset(&self, h: &Tipset) -> Result<Vec<UnsignedMessage>, Error> {
        chain::unsigned_messages_for_tipset(self.sm.blockstore(), h).map_err(|err| err.into())
    }

    fn load_tipset(&self, tsk: &TipsetKeys) -> Result<Tipset, Error> {
        let ts = chain::tipset_from_keys(self.sm.blockstore(), tsk)?;
        Ok(ts)
    }
    fn chain_compute_base_fee(&self, ts: &Tipset) -> Result<BigInt, Error> {
        chain::compute_base_fee(self.sm.blockstore(), ts).map_err(|err| err.into())
    }
    async fn state_account_key(&self, addr: &Address, ts: &Tipset) -> Result<Address, Error> {
        self.sm
            .resolve_to_key_addr(addr, ts)
            .await
            .map_err(|e| Error::Other(e.to_string()))
    }
}

/// This is the main MessagePool struct
pub struct MessagePool<T: 'static> {
    local_addrs: Arc<RwLock<Vec<Address>>>,
    pending: Arc<RwLock<HashMap<Address, MsgSet>>>,
    pub cur_tipset: Arc<RwLock<Tipset>>,
    api: Arc<RwLock<T>>,
    pub min_gas_price: BigInt,
    pub max_tx_pool_size: i64,
    pub network_name: String,
    bls_sig_cache: Arc<RwLock<LruCache<Cid, Signature>>>,
    sig_val_cache: Arc<RwLock<LruCache<Cid, ()>>>,
    // TODO look into adding a cap to local_msgs
    local_msgs: Arc<RwLock<HashSet<SignedMessage>>>,
    config: MpoolConfig,
}

impl<T> MessagePool<T>
where
    T: Provider + std::marker::Send + std::marker::Sync + 'static,
{
    /// Create a new message pool
    pub async fn new(
        mut api: T,
        network_name: String,
        config: MpoolConfig,
    ) -> Result<MessagePool<T>, Error>
    where
        T: Provider,
    {
        let local_addrs = Arc::new(RwLock::new(Vec::new()));
        // LruCache sizes have been taken from the lotus implementation
        let pending = Arc::new(RwLock::new(HashMap::new()));
        let tipset = Arc::new(RwLock::new(api.get_heaviest_tipset().await.ok_or_else(
            || Error::Other("No ts in api to set as cur_tipset".to_owned()),
        )?));
        let bls_sig_cache = Arc::new(RwLock::new(LruCache::new(40000)));
        let sig_val_cache = Arc::new(RwLock::new(LruCache::new(32000)));
        let api_mutex = Arc::new(RwLock::new(api));
        let local_msgs = Arc::new(RwLock::new(HashSet::new()));

        let mut mp = MessagePool {
            local_addrs,
            pending,
            cur_tipset: tipset,
            api: api_mutex,
            min_gas_price: Default::default(),
            max_tx_pool_size: 5000,
            network_name,
            bls_sig_cache,
            sig_val_cache,
            local_msgs,
            config,
        };

        mp.load_local().await?;

        let mut subscriber = mp.api.write().await.subscribe_head_changes().await;

        let api = mp.api.clone();
        let bls_sig_cache = mp.bls_sig_cache.clone();
        let pending = mp.pending.clone();

        // TODO: Check this
        let cur_tipset = mp.cur_tipset.clone();

        task::spawn(async move {
            loop {
                if let Some(ts) = subscriber.next().await {
                    let (cur, rev, app) = match ts {
                        HeadChange::Current(_tipset) => continue,
                        HeadChange::Revert(tipset) => (
                            cur_tipset.clone(),
                            vec![tipset.as_ref().clone()],
                            Vec::new(),
                        ),
                        HeadChange::Apply(tipset) => (
                            cur_tipset.clone(),
                            Vec::new(),
                            vec![tipset.as_ref().clone()],
                        ),
                    };
                    head_change(
                        api.as_ref(),
                        bls_sig_cache.as_ref(),
                        pending.as_ref(),
                        &cur.as_ref(),
                        rev,
                        app,
                    )
                    .await
                    .unwrap_or_else(|err| warn!("Error changing head: {:?}", err));
                }
            }
        });
        Ok(mp)
    }

    /// Add a signed message to local_addrs and local_msgs
    async fn add_local(&self, m: SignedMessage) -> Result<(), Error> {
        self.local_addrs.write().await.push(*m.from());
        self.local_msgs.write().await.insert(m);
        Ok(())
    }

    /// Push a signed message to the MessagePool
    pub async fn push(&self, msg: SignedMessage) -> Result<Cid, Error> {
        self.check_message(&msg).await?;
        let cid = msg.cid().map_err(|err| Error::Other(err.to_string()))?;
        self.add_tipset(msg.clone(), &self.cur_tipset.read().await.clone())
            .await?;
        self.add_local(msg).await?;
        // TODO: Publish over Gossip
        Ok(cid)
    }

    /// Basic checks on the validity of a message
    async fn check_message(&self, msg: &SignedMessage) -> Result<(), Error> {
        if msg.marshal_cbor()?.len() > 32 * 1024 {
            return Err(Error::MessageTooBig);
        }
        msg.valid_for_block_inclusion(0).map_err(Error::Other)?;
        if msg.value() > &BigInt::from(types::TOTAL_FILECOIN) {
            return Err(Error::MessageValueTooHigh);
        }
        if msg.gas_fee_cap() < &MINIMUM_BASE_FEE {
            return Err(Error::GasFeeCapTooLow);
        }
        self.verify_msg_sig(msg).await
    }

    /// This is a helper to push that will help to make sure that the message fits the parameters
    /// to be pushed to the MessagePool
    pub async fn add(&self, msg: &SignedMessage) -> Result<(), Error> {
        self.check_message(&msg).await?;
        let tmp = msg.clone();

        let tip = self.cur_tipset.read().await.clone();

        self.add_tipset(tmp, &tip).await
    }

    /// Add a SignedMessage without doing any of the checks
    pub async fn add_skip_checks(&mut self, m: SignedMessage) -> Result<(), Error> {
        self.add_helper(m).await
    }

    /// Verify the message signature. first check if it has already been verified and put into
    /// cache. If it has not, then manually verify it then put it into cache for future use
    async fn verify_msg_sig(&self, msg: &SignedMessage) -> Result<(), Error> {
        let cid = msg.cid()?;

        if let Some(()) = self.sig_val_cache.write().await.get(&cid) {
            return Ok(());
        }

        let umsg = msg.message().marshal_cbor()?;
        msg.signature()
            .verify(umsg.as_slice(), msg.from())
            .map_err(Error::Other)?;

        self.sig_val_cache.write().await.put(cid, ());

        Ok(())
    }

    /// Verify the state_sequence and balance for the sender of the message given then call add_locked
    /// to finish adding the signed_message to pending
    async fn add_tipset(&self, msg: SignedMessage, cur_ts: &Tipset) -> Result<(), Error> {
        let sequence = self.get_state_sequence(msg.from(), cur_ts).await?;

        if sequence > msg.message().sequence() {
            return Err(Error::SequenceTooLow);
        }

        let balance = self.get_state_balance(msg.from(), cur_ts).await?;

        let msg_balance = msg.message().required_funds();
        if balance < msg_balance {
            return Err(Error::NotEnoughFunds);
        }
        self.add_helper(msg).await
    }

    /// Finish verifying signed message before adding it to the pending mset hashmap. If an entry
    /// in the hashmap does not yet exist, create a new mset that will correspond to the from message
    /// and push it to the pending hashmap
    async fn add_helper(&self, msg: SignedMessage) -> Result<(), Error> {
        let from = *msg.from();
        add_helper(
            self.api.as_ref(),
            self.bls_sig_cache.as_ref(),
            self.pending.as_ref(),
            msg,
            self.get_state_sequence(&from, &self.cur_tipset.read().await.clone())
                .await?,
        )
        .await
    }

    /// Get the sequence for a given address, return Error if there is a failure to retrieve sequence
    pub async fn get_sequence(&self, addr: &Address) -> Result<u64, Error> {
        let cur_ts = self.cur_tipset.read().await.clone();

        let sequence = self.get_state_sequence(addr, &cur_ts).await?;

        let pending = self.pending.read().await;

        let msgset = pending.get(addr);
        match msgset {
            Some(mset) => {
                if sequence > mset.next_sequence {
                    return Ok(sequence);
                }
                Ok(mset.next_sequence)
            }
            None => Ok(sequence),
        }
    }

    /// Get the state of the sequence for a given address in cur_ts
    async fn get_state_sequence(&self, addr: &Address, cur_ts: &Tipset) -> Result<u64, Error> {
        let actor = self.api.read().await.get_actor_after(&addr, cur_ts)?;
        Ok(actor.sequence)
    }

    /// Get the state balance for the actor that corresponds to the supplied address and tipset,
    /// if this actor does not exist, return an error
    async fn get_state_balance(&self, addr: &Address, ts: &Tipset) -> Result<BigInt, Error> {
        let actor = self.api.read().await.get_actor_after(&addr, &ts)?;
        Ok(actor.balance)
    }

    /// Adds a local message returned from the call back function with the current nonce
    pub async fn push_with_sequence(&self, addr: &Address, cb: T) -> Result<SignedMessage, Error>
    where
        T: Fn(Address, u64) -> Result<SignedMessage, Error>,
    {
        let cur_ts = self.cur_tipset.read().await.clone();
        let from_key = match addr.protocol() {
            Protocol::ID => {
                let api = self.api.read().await;

                api.state_account_key(&addr, &self.cur_tipset.read().await.clone())
                    .await?
            }
            _ => *addr,
        };

        let sequence = self.get_sequence(&addr).await?;
        let msg = cb(from_key, sequence)?;
        self.check_message(&msg).await?;
        if *self.cur_tipset.read().await != cur_ts {
            return Err(Error::TryAgain);
        }

        if self.get_sequence(&addr).await? != sequence {
            return Err(Error::TryAgain);
        }

        let publish = verify_msg_before_add(&msg, &cur_ts, true)?;
        self.check_balance(&msg, &cur_ts).await?;
        self.add_helper(msg.clone()).await?;
        self.add_local(msg.clone()).await?;

        if publish {
            // TODO: Implement this, Publish message through gossipsub
        }

        Ok(msg)
    }

    async fn check_balance(&self, m: &SignedMessage, cur_ts: &Tipset) -> Result<(), Error> {
        let bal = self.get_state_balance(m.from(), &cur_ts).await?;
        let mut required_funds = m.required_funds();
        if bal < required_funds {
            return Err(Error::NotEnoughFunds);
        }
        if let Some(mset) = self.pending.read().await.get(m.from()) {
            required_funds += mset.get_required_funds(m.sequence());
        }
        if bal < required_funds {
            return Err(Error::SoftValidationFailure(format!(
                "not enough funds including pending messages (required: {}, balance: {})",
                required_funds, bal
            )));
        }
        Ok(())
    }

    /// Remove a message given a sequence and address from the messagepool
    pub async fn remove(
        &mut self,
        from: &Address,
        sequence: u64,
        applied: bool,
    ) -> Result<(), Error> {
        remove(from, self.pending.as_ref(), sequence, applied).await
    }

    /// Return a tuple that contains a vector of all signed messages and the current tipset for
    /// self.
    pub async fn pending(&self) -> Result<(Vec<SignedMessage>, Tipset), Error> {
        let mut out: Vec<SignedMessage> = Vec::new();
        let pending = self.pending.read().await;
        let pending_hm = pending.clone();

        for (addr, _) in pending_hm {
            out.append(
                self.pending_for(&addr)
                    .await
                    .ok_or_else(|| Error::InvalidFromAddr)?
                    .as_mut(),
            )
        }

        let cur_ts = self.cur_tipset.read().await.clone();

        Ok((out, cur_ts))
    }

    /// Return a Vector of signed messages for a given from address. This vector will be sorted by
    /// each messsage's sequence. If no corresponding messages found, return None result type
    pub async fn pending_for(&self, a: &Address) -> Option<Vec<SignedMessage>> {
        let pending = self.pending.read().await;
        let mset = pending.get(a)?;
        if mset.msgs.is_empty() {
            return None;
        }
        let mut msg_vec = Vec::new();
        for (_, item) in mset.msgs.iter() {
            msg_vec.push(item.clone());
        }
        msg_vec.sort_by_key(|value| value.message().sequence());
        Some(msg_vec)
    }

    /// Return Vector of signed messages given a block header for self
    pub async fn messages_for_blocks(
        &self,
        blks: &[BlockHeader],
    ) -> Result<Vec<SignedMessage>, Error> {
        let mut msg_vec: Vec<SignedMessage> = Vec::new();

        for block in blks {
            let (umsg, mut smsgs) = self.api.read().await.messages_for_block(&block)?;

            msg_vec.append(smsgs.as_mut());
            for msg in umsg {
                let mut bls_sig_cache = self.bls_sig_cache.write().await;
                let smsg = recover_sig(&mut bls_sig_cache, msg).await?;
                msg_vec.push(smsg)
            }
        }
        Ok(msg_vec)
    }

    /// Return gas price estimate this has been translated from lotus, a more smart implementation will
    /// most likely need to be implemented
    // TODO: UPDATE
    pub fn estimate_gas_premium(
        &self,
        nblocksincl: u64,
        _sender: Address,
        _gas_limit: u64,
        _tsk: TipsetKeys,
    ) -> Result<BigInt, Error> {
        // TODO possibly come up with a smarter way to estimate the gas price
        // TODO a smarter way exists now
        let min_gas_price = 0;
        match nblocksincl {
            0 => Ok(BigInt::from(min_gas_price + 2)),
            1 => Ok(BigInt::from(min_gas_price + 1)),
            _ => Ok(BigInt::from(min_gas_price)),
        }
    }

    /// local_message field
    pub async fn load_local(&mut self) -> Result<(), Error> {
        let mut local_msgs = self.local_msgs.write().await;
        let mut rm_vec = Vec::new();
        let msg_vec: Vec<SignedMessage> = local_msgs.iter().cloned().collect();

        for k in msg_vec {
            self.add(&k).await.unwrap_or_else(|err| {
                if err == Error::SequenceTooLow {
                    warn!("error adding message: {:?}", err);
                    rm_vec.push(k);
                }
            })
        }

        for item in rm_vec {
            local_msgs.remove(&item);
        }

        Ok(())
    }
    /// If `local = true`, the local messages will be removed as well as pending messages.
    /// If `local = false`, pending messages will be removed while retaining local messages.
    pub async fn clear(&mut self, local: bool) {
        if local {
            let local_addrs = self.local_addrs.read().await;
            for a in local_addrs.iter() {
                if let Some(mset) = self.pending.read().await.get(&a) {
                    for m in mset.msgs.values() {
                        if !self.local_msgs.write().await.remove(&m) {
                            warn!("error deleting local message");
                        }
                    }
                }
            }
            self.pending.write().await.clear();
        } else {
            let mut pending = self.pending.write().await;
            let local_addrs = self.local_addrs.read().await;
            pending.retain(|a, _| local_addrs.contains(&a));
        }
    }
    pub fn get_config(&self) -> &MpoolConfig {
        &self.config
    }
    pub fn set_config<DB: Store>(&mut self, db: &DB, cfg: MpoolConfig) -> Result<(), Error> {
        cfg.save_config(db)
            .map_err(|e| Error::Other(e.to_string()))?;
        self.config = cfg;
        Ok(())
    }
}

fn verify_msg_before_add(m: &SignedMessage, cur_ts: &Tipset, local: bool) -> Result<bool, Error> {
    let epoch = cur_ts.epoch();
    let min_gas = interpreter::price_list_by_epoch(epoch).on_chain_message(m.marshal_cbor()?.len());
    m.valid_for_block_inclusion(min_gas.total())
        .map_err(Error::Other)?;
    if !cur_ts.blocks().is_empty() {
        let base_fee = cur_ts.blocks()[0].parent_base_fee();
        let base_fee_lower_bound =
            get_base_fee_lower_bound(base_fee, BASE_FEE_LOWER_BOUND_FACTOR_CONSERVATIVE);
        if m.gas_fee_cap() < &base_fee_lower_bound {
            if local {
                warn!("local message will not be immediately published because GasFeeCap doesn't meet the lower bound for inclusion in the next 20 blocks (GasFeeCap: {}, baseFeeLowerBound: {})",m.gas_fee_cap(), base_fee_lower_bound);
                return Ok(false);
            } else {
                return Err(Error::SoftValidationFailure(format!("GasFeeCap doesn't meet base fee lower bound for inclusion in the next 20 blocks (GasFeeCap: {}, baseFeeLowerBound:{})",
					m.gas_fee_cap(), base_fee_lower_bound)));
            }
        }
    }
    Ok(local)
}

fn get_base_fee_lower_bound(base_fee: &BigInt, factor: i64) -> BigInt {
    let base_fee_lower_bound = base_fee / factor;
    if base_fee_lower_bound < *MINIMUM_BASE_FEE {
        return MINIMUM_BASE_FEE.clone();
    }
    base_fee_lower_bound
}

/// Remove a message from pending given the from address and sequence
pub async fn remove(
    from: &Address,
    pending: &RwLock<HashMap<Address, MsgSet>>,
    sequence: u64,
    applied: bool,
) -> Result<(), Error> {
    let mut pending = pending.write().await;
    let mset = if let Some(mset) = pending.get_mut(from) {
        mset
    } else {
        return Ok(());
    };

    mset.rm(sequence, applied);

    if mset.msgs.is_empty() {
        pending.remove(from);
    }

    Ok(())
}

/// Attempt to get a signed message that corresponds to an unsigned message in bls_sig_cache
async fn recover_sig(
    bls_sig_cache: &mut LruCache<Cid, Signature>,
    msg: UnsignedMessage,
) -> Result<SignedMessage, Error> {
    let val = bls_sig_cache
        .get(&msg.cid()?)
        .ok_or_else(|| Error::Other("Could not recover sig".to_owned()))?;
    let smsg = SignedMessage::new_from_parts(msg, val.clone()).map_err(Error::Other)?;
    Ok(smsg)
}

/// Finish verifying signed message before adding it to the pending mset hashmap. If an entry
/// in the hashmap does not yet exist, create a new mset that will correspond to the from message
/// and push it to the pending hashmap
async fn add_helper<T>(
    api: &RwLock<T>,
    bls_sig_cache: &RwLock<LruCache<Cid, Signature>>,
    pending: &RwLock<HashMap<Address, MsgSet>>,
    msg: SignedMessage,
    sequence: u64,
) -> Result<(), Error>
where
    T: Provider,
{
    if msg.signature().signature_type() == SignatureType::BLS {
        bls_sig_cache
            .write()
            .await
            .put(msg.cid()?, msg.signature().clone());
    }

    if msg.message().gas_limit() > 100_000_000 {
        return Err(Error::Other(
            "given message has too high of a gas limit".to_string(),
        ));
    }

    api.read().await.put_message(&msg)?;

    let mut pending = pending.write().await;
    let msett = pending.get_mut(msg.message().from());
    match msett {
        Some(mset) => mset.add(msg)?,
        None => {
            let mut mset = MsgSet::new(sequence);
            let from = *msg.message().from();
            mset.add(msg)?;
            pending.insert(from, mset);
        }
    }

    Ok(())
}
/// Get the state of the base_sequence for a given address in cur_ts
async fn get_state_sequence<T>(
    api: &RwLock<T>,
    addr: &Address,
    cur_ts: &Tipset,
) -> Result<u64, Error>
where
    T: Provider,
{
    let actor = api.read().await.get_actor_after(&addr, cur_ts)?;
    let base_sequence = actor.sequence;

    Ok(base_sequence)
}

/// This function will revert and/or apply tipsets to the message pool. This function should be
/// called every time that there is a head change in the message pool
pub async fn head_change<T>(
    api: &RwLock<T>,
    bls_sig_cache: &RwLock<LruCache<Cid, Signature>>,
    pending: &RwLock<HashMap<Address, MsgSet>>,
    cur_tipset: &RwLock<Tipset>,
    revert: Vec<Tipset>,
    apply: Vec<Tipset>,
) -> Result<(), Error>
where
    T: Provider + 'static,
{
    let mut rmsgs: HashMap<Address, HashMap<u64, SignedMessage>> = HashMap::new();
    for ts in revert {
        let pts = api.write().await.load_tipset(ts.parents())?;
        *cur_tipset.write().await = pts;

        let mut msgs: Vec<SignedMessage> = Vec::new();
        for block in ts.blocks() {
            let (umsg, mut smsgs) = api.read().await.messages_for_block(&block)?;
            msgs.append(smsgs.as_mut());
            // TODO: Unsigned messages
            for msg in umsg {
                let mut bls_sig_cache = bls_sig_cache.write().await;
                let smsg = recover_sig(&mut bls_sig_cache, msg).await?;
                msgs.push(smsg)
            }
        }

        for msg in msgs {
            add(msg, rmsgs.borrow_mut());
        }
    }

    for ts in apply {
        for b in ts.blocks() {
            let (msgs, smsgs) = api.read().await.messages_for_block(b)?;

            for msg in smsgs {
                rm(msg.from(), pending, msg.sequence(), rmsgs.borrow_mut()).await?;
            }
            for msg in msgs {
                rm(msg.from(), pending, msg.sequence(), rmsgs.borrow_mut()).await?;
            }
        }
        *cur_tipset.write().await = ts;
    }
    for (_, hm) in rmsgs {
        for (_, msg) in hm {
            let sequence =
                get_state_sequence(api, &msg.from(), &cur_tipset.read().await.clone()).await?;
            if let Err(e) = add_helper(api, bls_sig_cache, pending, msg, sequence).await {
                error!("Failed to readd message from reorg to mpool: {}", e);
            }
        }
    }
    Ok(())
}

/// This is a helper method for head_change. This method will remove a sequence for a from address
/// from the rmsgs hashmap. Also remove the from address and sequence from the messagepool.
async fn rm(
    from: &Address,
    pending: &RwLock<HashMap<Address, MsgSet>>,
    sequence: u64,
    rmsgs: &mut HashMap<Address, HashMap<u64, SignedMessage>>,
) -> Result<(), Error> {
    if let Some(temp) = rmsgs.get_mut(from) {
        if temp.get_mut(&sequence).is_some() {
            temp.remove(&sequence);
        } else {
            remove(from, pending, sequence, true).await?;
        }
    } else {
        remove(from, pending, sequence, true).await?;
    }
    Ok(())
}

/// This function is a helper method for head_change. This method will add a signed message to the given rmsgs HashMap
fn add(m: SignedMessage, rmsgs: &mut HashMap<Address, HashMap<u64, SignedMessage>>) {
    let s = rmsgs.get_mut(m.from());
    if let Some(s) = s {
        s.insert(m.sequence(), m);
    } else {
        rmsgs.insert(*m.from(), HashMap::new());
        rmsgs.get_mut(m.from()).unwrap().insert(m.sequence(), m);
    }
}

pub mod test_provider {
    use super::Error as Errors;
    use super::*;
    use address::Address;
    use blocks::{BlockHeader, Tipset};
    use cid::Cid;
    use flo_stream::{MessagePublisher, Publisher, Subscriber};
    use message::{SignedMessage, UnsignedMessage};

    /// Struct used for creating a provider when writing tests involving message pool
    pub struct TestApi {
        bmsgs: HashMap<Cid, Vec<SignedMessage>>,
        state_sequence: HashMap<Address, u64>,
        tipsets: Vec<Tipset>,
        publisher: Publisher<HeadChange>,
    }

    impl Default for TestApi {
        /// Create a new TestApi
        fn default() -> Self {
            TestApi {
                bmsgs: HashMap::new(),
                state_sequence: HashMap::new(),
                tipsets: Vec::new(),
                publisher: Publisher::new(1),
            }
        }
    }

    impl TestApi {
        /// Set the state sequence for an Address for TestApi
        pub fn set_state_sequence(&mut self, addr: &Address, sequence: u64) {
            self.state_sequence.insert(*addr, sequence);
        }

        /// Set the block messages for TestApi
        pub fn set_block_messages(&mut self, h: &BlockHeader, msgs: Vec<SignedMessage>) {
            self.bmsgs.insert(h.cid().clone(), msgs);
            self.tipsets.push(Tipset::new(vec![h.clone()]).unwrap())
        }

        /// Set the heaviest tipset for TestApi
        pub async fn set_heaviest_tipset(&mut self, ts: Arc<Tipset>) {
            self.publisher.publish(HeadChange::Apply(ts)).await
        }
    }

    #[async_trait]
    impl Provider for TestApi {
        async fn subscribe_head_changes(&mut self) -> Subscriber<HeadChange> {
            self.publisher.subscribe()
        }

        async fn get_heaviest_tipset(&mut self) -> Option<Tipset> {
            Tipset::new(vec![create_header(1, b"", b"")]).ok()
        }

        fn put_message(&self, _msg: &SignedMessage) -> Result<Cid, Errors> {
            Ok(Cid::default())
        }

        fn get_actor_after(&self, addr: &Address, ts: &Tipset) -> Result<ActorState, Errors> {
            let mut msgs: Vec<SignedMessage> = Vec::new();
            for b in ts.blocks() {
                if let Some(ms) = self.bmsgs.get(b.cid()) {
                    for m in ms {
                        if m.from() == addr {
                            msgs.push(m.clone());
                        }
                    }
                }
            }
            msgs.sort_by_key(|m| m.sequence());
            let mut sequence: u64 = self.state_sequence.get(addr).copied().unwrap_or_default();
            for m in msgs {
                if m.sequence() != sequence {
                    break;
                }
                sequence += 1;
            }
            let actor = ActorState::new(
                Cid::default(),
                Cid::default(),
                // TODO balance not handled in tests
                BigInt::from(9_000_000 as u64),
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

        async fn state_account_key(&self, addr: &Address, _ts: &Tipset) -> Result<Address, Error> {
            match addr.protocol() {
                Protocol::BLS | Protocol::Secp256k1 => Ok(*addr),
                _ => Err(Error::Other("given address was not a key addr".to_string())),
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

        fn chain_compute_base_fee(&self, _ts: &Tipset) -> Result<BigInt, Error> {
            Ok(100.into())
        }
    }

    pub fn create_header(weight: u64, parent_bz: &[u8], cached_bytes: &[u8]) -> BlockHeader {
        BlockHeader::builder()
            .weight(BigInt::from(weight))
            .cached_bytes(cached_bytes.to_vec())
            .cached_cid(Cid::new_from_cbor(parent_bz, Blake2b256))
            .miner_address(Address::new_id(0))
            .build()
            .unwrap()
    }
}

#[cfg(test)]
pub mod tests {
    use super::test_provider::*;
    use super::*;
    use crate::MessagePool;
    use address::Address;
    use async_std::task;
    use blocks::{BlockHeader, ElectionProof, Ticket, Tipset};
    use cid::Cid;
    use crypto::{SignatureType, VRFProof};
    use key_management::{MemKeyStore, Wallet};
    use message::{SignedMessage, UnsignedMessage};
    use num_bigint::BigInt;
    use std::borrow::BorrowMut;
    use std::convert::TryFrom;
    use std::thread::sleep;
    use std::time::Duration;

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
            .gas_fee_cap(100.into())
            .build()
            .unwrap();
        let message_cbor = Cbor::marshal_cbor(&umsg).unwrap();
        let sig = wallet.sign(&from, message_cbor.as_slice()).unwrap();
        SignedMessage::new_from_parts(umsg, sig).unwrap()
    }

    fn mock_block(weight: u64, ticket_sequence: u64) -> BlockHeader {
        let addr = Address::new_id(1234561);
        let c =
            Cid::try_from("bafyreicmaj5hhoy5mgqvamfhgexxyergw7hdeshizghodwkjg6qmpoco7i").unwrap();

        let fmt_str = format!("===={}=====", ticket_sequence);
        let ticket = Ticket::new(VRFProof::new(fmt_str.clone().into_bytes()));
        let election_proof = ElectionProof {
            win_count: 0,
            vrfproof: VRFProof::new(fmt_str.into_bytes()),
        };
        let weight_inc = BigInt::from(weight);
        BlockHeader::builder()
            .miner_address(addr)
            .election_proof(Some(election_proof))
            .ticket(Some(ticket))
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

        let mut weight_inc = BigInt::from(weight);
        weight_inc = parents.blocks()[0].weight() + weight_inc;
        let fmt_str = format!("===={}=====", ticket_sequence);
        let ticket = Ticket::new(VRFProof::new(fmt_str.clone().into_bytes()));
        let election_proof = ElectionProof {
            win_count: 0,
            vrfproof: VRFProof::new(fmt_str.into_bytes()),
        };
        BlockHeader::builder()
            .miner_address(addr)
            .election_proof(Some(election_proof))
            .ticket(Some(ticket))
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

        let mut tma = TestApi::default();
        tma.set_state_sequence(&sender, 0);

        task::block_on(async move {
            let mpool = MessagePool::new(tma, "mptest".to_string(), Default::default())
                .await
                .unwrap();
            let mut smsg_vec = Vec::new();
            for i in 0..2 {
                let msg = create_smsg(&target, &sender, wallet.borrow_mut(), i);
                smsg_vec.push(msg);
            }

            mpool.api.write().await.set_state_sequence(&sender, 0);
            assert_eq!(mpool.get_sequence(&sender).await.unwrap(), 0);
            mpool.add(&smsg_vec[0].clone()).await.unwrap();
            assert_eq!(mpool.get_sequence(&sender).await.unwrap(), 1);
            mpool.add(&smsg_vec[1].clone()).await.unwrap();
            assert_eq!(mpool.get_sequence(&sender).await.unwrap(), 2);

            let a = mock_block(1, 1);

            mpool.api.write().await.set_block_messages(&a, smsg_vec);
            let api = mpool.api.clone();
            let bls_sig_cache = mpool.bls_sig_cache.clone();
            let pending = mpool.pending.clone();
            let cur_tipset = mpool.cur_tipset.clone();

            head_change(
                api.as_ref(),
                bls_sig_cache.as_ref(),
                pending.as_ref(),
                cur_tipset.as_ref(),
                Vec::new(),
                vec![Tipset::new(vec![a]).unwrap()],
            )
            .await
            .unwrap();

            assert_eq!(mpool.get_sequence(&sender).await.unwrap(), 2);
        })
    }

    #[test]
    fn test_revert_messages() {
        let tma = TestApi::default();
        let mut wallet = Wallet::new(MemKeyStore::new());

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

        task::block_on(async move {
            let mpool = MessagePool::new(tma, "mptest".to_string(), Default::default())
                .await
                .unwrap();

            let mut api_temp = mpool.api.write().await;
            api_temp.set_block_messages(&a, vec![smsg_vec[0].clone()]);
            api_temp.set_block_messages(&b.clone(), smsg_vec[1..4].to_vec());
            api_temp.set_state_sequence(&sender, 0);
            drop(api_temp);

            mpool.add(&smsg_vec[0]).await.unwrap();
            mpool.add(&smsg_vec[1]).await.unwrap();
            mpool.add(&smsg_vec[2]).await.unwrap();
            mpool.add(&smsg_vec[3]).await.unwrap();

            mpool.api.write().await.set_state_sequence(&sender, 0);

            let api = mpool.api.clone();
            let bls_sig_cache = mpool.bls_sig_cache.clone();
            let pending = mpool.pending.clone();
            let cur_tipset = mpool.cur_tipset.clone();

            head_change(
                api.as_ref(),
                bls_sig_cache.as_ref(),
                pending.as_ref(),
                cur_tipset.as_ref(),
                Vec::new(),
                vec![Tipset::new(vec![a]).unwrap()],
            )
            .await
            .unwrap();

            assert_eq!(mpool.get_sequence(&sender).await.unwrap(), 4);

            mpool.api.write().await.set_state_sequence(&sender, 1);

            let api = mpool.api.clone();
            let bls_sig_cache = mpool.bls_sig_cache.clone();
            let pending = mpool.pending.clone();
            let cur_tipset = mpool.cur_tipset.clone();

            head_change(
                api.as_ref(),
                bls_sig_cache.as_ref(),
                pending.as_ref(),
                cur_tipset.as_ref(),
                Vec::new(),
                vec![Tipset::new(vec![b.clone()]).unwrap()],
            )
            .await
            .unwrap();

            assert_eq!(mpool.get_sequence(&sender).await.unwrap(), 4);

            mpool.api.write().await.set_state_sequence(&sender, 0);

            head_change(
                api.as_ref(),
                bls_sig_cache.as_ref(),
                pending.as_ref(),
                cur_tipset.as_ref(),
                vec![Tipset::new(vec![b]).unwrap()],
                Vec::new(),
            )
            .await
            .unwrap();

            assert_eq!(mpool.get_sequence(&sender).await.unwrap(), 4);

            let (p, _) = mpool.pending().await.unwrap();
            assert_eq!(p.len(), 3);
        })
    }

    #[test]
    fn test_async_message_pool() {
        let keystore = MemKeyStore::new();
        let mut wallet = Wallet::new(keystore);
        let sender = wallet.generate_addr(SignatureType::Secp256k1).unwrap();
        let target = wallet.generate_addr(SignatureType::Secp256k1).unwrap();

        let mut tma = TestApi::default();
        tma.set_state_sequence(&sender, 0);

        task::block_on(async move {
            let mpool = MessagePool::new(tma, "mptest".to_string(), Default::default())
                .await
                .unwrap();

            let mut smsg_vec = Vec::new();
            for i in 0..3 {
                let msg = create_smsg(&target, &sender, wallet.borrow_mut(), i);
                smsg_vec.push(msg);
            }

            assert_eq!(mpool.get_sequence(&sender).await.unwrap(), 0);
            mpool.push(smsg_vec[0].clone()).await.unwrap();
            assert_eq!(mpool.get_sequence(&sender).await.unwrap(), 1);
            mpool.push(smsg_vec[1].clone()).await.unwrap();
            assert_eq!(mpool.get_sequence(&sender).await.unwrap(), 2);
            mpool.push(smsg_vec[2].clone()).await.unwrap();
            assert_eq!(mpool.get_sequence(&sender).await.unwrap(), 3);

            let header = mock_block(1, 1);
            let tipset = Tipset::new(vec![header.clone()]).unwrap();

            let ts = tipset.clone();
            mpool
                .api
                .write()
                .await
                .set_heaviest_tipset(Arc::new(ts))
                .await;

            // sleep allows for async block to update mpool's cur_tipset
            sleep(Duration::new(2, 0));

            let cur_ts = mpool.cur_tipset.read().await.clone();
            assert_eq!(cur_ts, tipset);
        })
    }
}
