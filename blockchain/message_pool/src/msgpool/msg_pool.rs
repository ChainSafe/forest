// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

// Contains the implementation of Message Pool component.
// The Message Pool is the component of forest that handles pending messages for inclusion
// in the chain. Messages are added either directly for locally published messages
// or through pubsub propagation.

use crate::config::MpoolConfig;
use crate::errors::Error;
use crate::head_change;
use crate::msgpool::recover_sig;
use crate::msgpool::republish_pending_messages;
use crate::msgpool::BASE_FEE_LOWER_BOUND_FACTOR_CONSERVATIVE;
use crate::msgpool::REPUBLISH_INTERVAL;
use crate::msgpool::{RBF_DENOM, RBF_NUM};
use crate::provider::Provider;
use crate::utils::get_base_fee_lower_bound;
use address::{Address, Protocol};
use async_std::channel::{bounded, Sender};
use async_std::stream::interval;
use async_std::sync::{Arc, RwLock};
use async_std::task;
use blocks::{BlockHeader, Tipset, TipsetKeys};
use chain::{HeadChange, MINIMUM_BASE_FEE};
use cid::Cid;
use crypto::{Signature, SignatureType};
use db::Store;
use encoding::Cbor;
use forest_libp2p::{NetworkMessage, Topic, PUBSUB_MSG_STR};
use futures::{future::select, StreamExt};
use log::warn;
use lru::LruCache;
use message::{ChainMessage, Message, SignedMessage};
use networks::NEWEST_NETWORK_VERSION;
use num_bigint::BigInt;
use num_bigint::Integer;
use std::collections::{HashMap, HashSet};
use std::time::Duration;
use tokio::sync::broadcast::error::RecvError;
use types::verifier::ProofVerifier;

// LruCache sizes have been taken from the lotus implementation
const BLS_SIG_CACHE_SIZE: usize = 40000;
const SIG_VAL_CACHE_SIZE: usize = 32000;

/// Simple struct that contains a hashmap of messages where k: a message from address, v: a message
/// which corresponds to that address.
#[derive(Clone, Default, Debug)]
pub struct MsgSet {
    pub(crate) msgs: HashMap<u64, SignedMessage>,
    next_sequence: u64,
    required_funds: BigInt,
}

impl MsgSet {
    /// Generate a new MsgSet with an empty hashmap and setting the sequence specifically.
    pub fn new(sequence: u64) -> Self {
        MsgSet {
            msgs: HashMap::new(),
            next_sequence: sequence,
            required_funds: Default::default(),
        }
    }

    /// Add a signed message to the MsgSet. Increase next_sequence if the message has a
    /// sequence greater than any existing message sequence.
    pub fn add(&mut self, m: SignedMessage) -> Result<(), Error> {
        if self.msgs.is_empty() || m.sequence() >= self.next_sequence {
            self.next_sequence = m.sequence() + 1;
        }
        if let Some(exms) = self.msgs.get(&m.sequence()) {
            if m.cid()? != exms.cid()? {
                let premium = exms.message().gas_premium();
                let rbf_denom = BigInt::from(RBF_DENOM);
                let min_price = premium + ((premium * RBF_NUM).div_floor(&rbf_denom)) + 1u8;
                if m.message().gas_premium() <= &min_price {
                    return Err(Error::GasPriceTooLow);
                }
            } else {
                return Err(Error::DuplicateSequence);
            }
        }
        self.msgs.insert(m.sequence(), m);
        Ok(())
    }

    /// Removes message with the given sequence. If applied, update the set's next sequence.
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

/// This contains all necessary information needed for the message pool.
/// Keeps track of messages to apply, as well as context needed for verifying transactions.
pub struct MessagePool<T> {
    /// The local address of the client
    local_addrs: Arc<RwLock<Vec<Address>>>,
    /// A map of pending messages where the key is the address
    pub pending: Arc<RwLock<HashMap<Address, MsgSet>>>,
    /// The current tipset (a set of blocks)
    pub cur_tipset: Arc<RwLock<Arc<Tipset>>>,
    /// The underlying provider
    pub api: Arc<RwLock<T>>,
    /// The minimum gas price needed for executing the transaction based on number of included blocks
    pub min_gas_price: BigInt,
    /// This is max number of messages in the pool.
    pub max_tx_pool_size: i64,
    /// TODO
    pub network_name: String,
    /// Sender half to send messages to other components
    pub network_sender: Sender<NetworkMessage>,
    /// A cache for BLS signature keyed by Cid
    pub bls_sig_cache: Arc<RwLock<LruCache<Cid, Signature>>>,
    /// A cache for BLS signature keyed by Cid
    pub sig_val_cache: Arc<RwLock<LruCache<Cid, ()>>>,
    /// A set of republished messages identified by their Cid
    pub republished: Arc<RwLock<HashSet<Cid>>>,
    /// Acts as a signal to republish messages from the republished set of messages
    pub repub_trigger: Sender<()>,
    /// TODO look into adding a cap to local_msgs
    local_msgs: Arc<RwLock<HashSet<SignedMessage>>>,
    /// Configurable parameters of the message pool
    pub config: MpoolConfig,
}

impl<T> MessagePool<T>
where
    T: Provider + std::marker::Send + std::marker::Sync + 'static,
{
    /// Creates a new MessagePool instance.
    pub async fn new(
        mut api: T,
        network_name: String,
        network_sender: Sender<NetworkMessage>,
        config: MpoolConfig,
    ) -> Result<MessagePool<T>, Error>
    where
        T: Provider,
    {
        let local_addrs = Arc::new(RwLock::new(Vec::new()));
        let pending = Arc::new(RwLock::new(HashMap::new()));
        let tipset = Arc::new(RwLock::new(api.get_heaviest_tipset().await.ok_or_else(
            || Error::Other("Failed to retrieve heaviest tipset from provider".to_owned()),
        )?));
        let bls_sig_cache = Arc::new(RwLock::new(LruCache::new(BLS_SIG_CACHE_SIZE)));
        let sig_val_cache = Arc::new(RwLock::new(LruCache::new(SIG_VAL_CACHE_SIZE)));
        let api_mutex = Arc::new(RwLock::new(api));
        let local_msgs = Arc::new(RwLock::new(HashSet::new()));
        let republished = Arc::new(RwLock::new(HashSet::new()));

        let (repub_trigger, mut repub_trigger_rx) = bounded::<()>(4);
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
            republished,
            config,
            network_sender,
            repub_trigger,
        };

        mp.load_local().await?;

        let mut subscriber = mp.api.write().await.subscribe_head_changes().await;

        let api = mp.api.clone();
        let bls_sig_cache = mp.bls_sig_cache.clone();
        let pending = mp.pending.clone();
        let republished = mp.republished.clone();

        let cur_tipset = mp.cur_tipset.clone();
        let repub_trigger = Arc::new(mp.repub_trigger.clone());

        // Reacts to new HeadChanges
        task::spawn(async move {
            loop {
                match subscriber.recv().await {
                    Ok(ts) => {
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
                            repub_trigger.clone(),
                            republished.as_ref(),
                            pending.as_ref(),
                            &cur.as_ref(),
                            rev,
                            app,
                        )
                        .await
                        .unwrap_or_else(|err| warn!("Error changing head: {:?}", err));
                    }
                    Err(RecvError::Lagged(e)) => {
                        warn!("Head change subscriber lagged: skipping {} events", e);
                    }
                    Err(RecvError::Closed) => {
                        break;
                    }
                }
            }
        });

        let api = mp.api.clone();
        let pending = mp.pending.clone();
        let cur_tipset = mp.cur_tipset.clone();
        let republished = mp.republished.clone();
        let local_addrs = mp.local_addrs.clone();
        let network_sender = Arc::new(mp.network_sender.clone());
        let network_name = mp.network_name.clone();
        // Reacts to republishing requests
        task::spawn(async move {
            let mut interval = interval(Duration::from_millis(REPUBLISH_INTERVAL));
            loop {
                select(interval.next(), repub_trigger_rx.next()).await;
                if let Err(e) = republish_pending_messages(
                    api.as_ref(),
                    network_sender.as_ref(),
                    network_name.as_ref(),
                    pending.as_ref(),
                    cur_tipset.as_ref(),
                    republished.as_ref(),
                    local_addrs.as_ref(),
                )
                .await
                {
                    warn!("Failed to republish pending messages: {}", e.to_string());
                }
            }
        });
        Ok(mp)
    }

    /// Add a signed message to the pool and its address.
    async fn add_local(&self, m: SignedMessage) -> Result<(), Error> {
        self.local_addrs.write().await.push(*m.from());
        self.local_msgs.write().await.insert(m);
        Ok(())
    }

    /// Push a signed message to the MessagePool. Additionally performs
    pub async fn push(&self, msg: SignedMessage) -> Result<Cid, Error> {
        self.check_message(&msg).await?;
        let cid = msg.cid().map_err(|err| Error::Other(err.to_string()))?;
        let cur_ts = self.cur_tipset.read().await.clone();
        let publish = self.add_tipset(msg.clone(), &cur_ts, true).await?;
        let msg_ser = msg.marshal_cbor()?;
        self.add_local(msg).await?;
        if publish {
            self.network_sender
                .send(NetworkMessage::PubsubMessage {
                    topic: Topic::new(format!("{}/{}", PUBSUB_MSG_STR, self.network_name)),
                    message: msg_ser,
                })
                .await
                .map_err(|_| Error::Other("Network receiver dropped".to_string()))?;
        }
        Ok(cid)
    }

    /// Basic checks on the validity of a message.
    async fn check_message(&self, msg: &SignedMessage) -> Result<(), Error> {
        if msg.marshal_cbor()?.len() > 32 * 1024 {
            return Err(Error::MessageTooBig);
        }
        msg.message()
            .valid_for_block_inclusion(0, NEWEST_NETWORK_VERSION)
            .map_err(Error::Other)?;
        if msg.value() > &types::TOTAL_FILECOIN {
            return Err(Error::MessageValueTooHigh);
        }
        if msg.gas_fee_cap() < &MINIMUM_BASE_FEE {
            return Err(Error::GasFeeCapTooLow);
        }
        self.verify_msg_sig(msg).await
    }

    /// This is a helper to push that will help to make sure that the message fits the parameters
    /// to be pushed to the MessagePool.
    pub async fn add(&self, msg: SignedMessage) -> Result<(), Error> {
        self.check_message(&msg).await?;

        let tip = self.cur_tipset.read().await.clone();

        self.add_tipset(msg, &tip, false).await?;
        Ok(())
    }

    /// Add a SignedMessage without doing any of the checks.
    pub async fn add_skip_checks(&mut self, m: SignedMessage) -> Result<(), Error> {
        self.add_helper(m).await
    }

    /// Verify the message signature. first check if it has already been verified and put into
    /// cache. If it has not, then manually verify it then put it into cache for future use.
    async fn verify_msg_sig(&self, msg: &SignedMessage) -> Result<(), Error> {
        let cid = msg.cid()?;

        if let Some(()) = self.sig_val_cache.write().await.get(&cid) {
            return Ok(());
        }

        msg.verify().map_err(Error::Other)?;

        self.sig_val_cache.write().await.put(cid, ());

        Ok(())
    }

    /// Verify the state_sequence and balance for the sender of the message given then
    /// call add_locked to finish adding the signed_message to pending.
    async fn add_tipset(
        &self,
        msg: SignedMessage,
        cur_ts: &Tipset,
        local: bool,
    ) -> Result<bool, Error> {
        let sequence = self.get_state_sequence(msg.from(), cur_ts).await?;

        if sequence > msg.message().sequence() {
            return Err(Error::SequenceTooLow);
        }

        let publish = verify_msg_before_add(&msg, &cur_ts, local)?;

        let balance = self.get_state_balance(msg.from(), cur_ts).await?;

        let msg_balance = msg.message().required_funds();
        if balance < msg_balance {
            return Err(Error::NotEnoughFunds);
        }
        self.add_helper(msg).await?;
        Ok(publish)
    }

    /// Finish verifying signed message before adding it to the pending mset hashmap. If an entry
    /// in the hashmap does not yet exist, create a new mset that will correspond to the from
    /// message and push it to the pending hashmap.
    async fn add_helper(&self, msg: SignedMessage) -> Result<(), Error> {
        let from = *msg.from();
        let cur_ts = self.cur_tipset.read().await.clone();
        add_helper(
            self.api.as_ref(),
            self.bls_sig_cache.as_ref(),
            self.pending.as_ref(),
            msg,
            self.get_state_sequence(&from, &cur_ts).await?,
        )
        .await
    }

    /// Get the sequence for a given address, return Error if there is a failure to retrieve
    /// the respective sequence.
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

    /// Get the state of the sequence for a given address in cur_ts.
    async fn get_state_sequence(&self, addr: &Address, cur_ts: &Tipset) -> Result<u64, Error> {
        let actor = self.api.read().await.get_actor_after(&addr, cur_ts)?;
        Ok(actor.sequence)
    }

    /// Get the state balance for the actor that corresponds to the supplied address and tipset,
    /// if this actor does not exist, return an error.
    async fn get_state_balance(&self, addr: &Address, ts: &Tipset) -> Result<BigInt, Error> {
        let actor = self.api.read().await.get_actor_after(&addr, &ts)?;
        Ok(actor.balance)
    }

    /// Adds a local message returned from the call back function with the current nonce.
    pub async fn push_with_sequence<V>(&self, addr: &Address, cb: T) -> Result<SignedMessage, Error>
    where
        T: Fn(Address, u64) -> Result<SignedMessage, Error>,
        V: ProofVerifier,
    {
        let cur_ts = self.cur_tipset.read().await.clone();
        let from_key = match addr.protocol() {
            Protocol::ID => {
                let api = self.api.read().await;

                api.state_account_key::<V>(&addr, &self.cur_tipset.read().await.clone())
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
            self.network_sender
                .send(NetworkMessage::PubsubMessage {
                    topic: Topic::new(format!("{}/{}", PUBSUB_MSG_STR, self.network_name)),
                    message: msg.marshal_cbor()?,
                })
                .await
                .map_err(|_| Error::Other("Network receiver dropped".to_string()))?;
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

    /// Remove a message given a sequence and address from the messagepool.
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
    pub async fn pending(&self) -> Result<(Vec<SignedMessage>, Arc<Tipset>), Error> {
        let mut out: Vec<SignedMessage> = Vec::new();
        let pending = self.pending.read().await;
        let pending_hm = pending.clone();

        for (addr, _) in pending_hm {
            out.append(
                self.pending_for(&addr)
                    .await
                    .ok_or(Error::InvalidFromAddr)?
                    .as_mut(),
            )
        }

        let cur_ts = self.cur_tipset.read().await.clone();

        Ok((out, cur_ts))
    }

    /// Return a Vector of signed messages for a given from address. This vector will be sorted by
    /// each messsage's sequence. If no corresponding messages found, return None result type.
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

    /// Return Vector of signed messages given a block header for self.
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
    /// most likely need to be implemented.
    // TODO: UPDATE https://github.com/ChainSafe/forest/issues/901
    pub fn estimate_gas_premium(
        &self,
        nblocksincl: u64,
        _sender: Address,
        _gas_limit: u64,
        _tsk: TipsetKeys,
    ) -> Result<BigInt, Error> {
        let min_gas_price = 0;
        match nblocksincl {
            0 => Ok(BigInt::from(min_gas_price + 2)),
            1 => Ok(BigInt::from(min_gas_price + 1)),
            _ => Ok(BigInt::from(min_gas_price)),
        }
    }

    /// Loads local messages to the message pool to be applied.
    pub async fn load_local(&mut self) -> Result<(), Error> {
        let mut local_msgs = self.local_msgs.write().await;
        let msg_vec: Vec<SignedMessage> = local_msgs.iter().cloned().collect();

        for k in msg_vec.into_iter() {
            self.add(k.clone()).await.unwrap_or_else(|err| {
                if err == Error::SequenceTooLow {
                    warn!("error adding message: {:?}", err);
                    local_msgs.remove(&k);
                }
            })
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
            self.republished.write().await.clear();
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

// Helpers for MessagePool

/// Finish verifying signed message before adding it to the pending mset hashmap. If an entry
/// in the hashmap does not yet exist, create a new mset that will correspond to the from message
/// and push it to the pending hashmap.
pub(crate) async fn add_helper<T>(
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

    api.read()
        .await
        .put_message(&ChainMessage::Signed(msg.clone()))?;
    api.read()
        .await
        .put_message(&ChainMessage::Unsigned(msg.message().clone()))?;

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

fn verify_msg_before_add(m: &SignedMessage, cur_ts: &Tipset, local: bool) -> Result<bool, Error> {
    let epoch = cur_ts.epoch();
    let min_gas = interpreter::price_list_by_epoch(epoch).on_chain_message(m.marshal_cbor()?.len());
    m.message()
        .valid_for_block_inclusion(min_gas.total(), NEWEST_NETWORK_VERSION)
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

/// Remove a message from pending given the from address and sequence.
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
