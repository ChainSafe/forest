// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

// Contains the implementation of Message Pool component.
// The Message Pool is the component of forest that handles pending messages for
// inclusion in the chain. Messages are added either directly for locally
// published messages or through pubsub propagation.

use std::{num::NonZeroUsize, sync::Arc, time::Duration};

use crate::blocks::{BlockHeader, Tipset};
use crate::chain::{HeadChange, MINIMUM_BASE_FEE};
#[cfg(test)]
use crate::db::Store;
use crate::libp2p::{NetworkMessage, Topic, PUBSUB_MSG_STR};
use crate::message::{valid_for_block_inclusion, ChainMessage, Message, SignedMessage};
use crate::networks::{ChainConfig, NEWEST_NETWORK_VERSION};
use crate::shim::{
    address::Address,
    crypto::{Signature, SignatureType},
    econ::TokenAmount,
    gas::{price_list_by_network_version, Gas},
};
use crate::state_manager::is_valid_for_sending;
use ahash::{HashMap, HashMapExt, HashSet, HashSetExt};
use anyhow::Context;
use cid::Cid;
use futures::StreamExt;
use fvm_ipld_encoding::to_vec;
use log::warn;
use lru::LruCache;
use nonzero_ext::nonzero;
use num::BigInt;
use parking_lot::{Mutex, RwLock as SyncRwLock};
use tokio::{sync::broadcast::error::RecvError, task::JoinSet, time::interval};

use crate::message_pool::{
    config::MpoolConfig,
    errors::Error,
    head_change, metrics,
    msgpool::{
        recover_sig, republish_pending_messages, select_messages_for_block,
        BASE_FEE_LOWER_BOUND_FACTOR_CONSERVATIVE, RBF_DENOM, RBF_NUM,
    },
    provider::Provider,
    utils::get_base_fee_lower_bound,
};

// LruCache sizes have been taken from the lotus implementation
const BLS_SIG_CACHE_SIZE: NonZeroUsize = nonzero!(40000usize);
const SIG_VAL_CACHE_SIZE: NonZeroUsize = nonzero!(32000usize);

/// Simple structure that contains a hash-map of messages where k: a message
/// from address, v: a message which corresponds to that address.
#[derive(Clone, Default, Debug)]
pub struct MsgSet {
    pub(in crate::message_pool) msgs: HashMap<u64, SignedMessage>,
    next_sequence: u64,
}

impl MsgSet {
    /// Generate a new `MsgSet` with an empty hash-map and setting the sequence
    /// specifically.
    pub fn new(sequence: u64) -> Self {
        MsgSet {
            msgs: HashMap::new(),
            next_sequence: sequence,
        }
    }

    /// Add a signed message to the `MsgSet`. Increase `next_sequence` if the
    /// message has a sequence greater than any existing message sequence.
    pub fn add(&mut self, m: SignedMessage) -> Result<(), Error> {
        if self.msgs.is_empty() || m.sequence() >= self.next_sequence {
            self.next_sequence = m.sequence() + 1;
        }
        if let Some(exms) = self.msgs.get(&m.sequence()) {
            if m.cid()? != exms.cid()? {
                let premium = &exms.message().gas_premium;
                let min_price = premium.clone()
                    + ((premium * RBF_NUM).div_floor(RBF_DENOM))
                    + TokenAmount::from_atto(1u8);
                if m.message().gas_premium <= min_price {
                    return Err(Error::GasPriceTooLow);
                }
            } else {
                return Err(Error::DuplicateSequence);
            }
        }
        if self.msgs.insert(m.sequence(), m).is_none() {
            metrics::MPOOL_MESSAGE_TOTAL.inc();
        }
        Ok(())
    }

    /// Removes message with the given sequence. If applied, update the set's
    /// next sequence.
    pub fn rm(&mut self, sequence: u64, applied: bool) {
        if self.msgs.remove(&sequence).is_none() {
            if applied && sequence >= self.next_sequence {
                self.next_sequence = sequence + 1;
                while self.msgs.get(&self.next_sequence).is_some() {
                    self.next_sequence += 1;
                }
            }
            return;
        }
        metrics::MPOOL_MESSAGE_TOTAL.dec();

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
}

/// This contains all necessary information needed for the message pool.
/// Keeps track of messages to apply, as well as context needed for verifying
/// transactions.
pub struct MessagePool<T> {
    /// The local address of the client
    local_addrs: Arc<SyncRwLock<Vec<Address>>>,
    /// A map of pending messages where the key is the address
    pub pending: Arc<SyncRwLock<HashMap<Address, MsgSet>>>,
    /// The current tipset (a set of blocks)
    pub cur_tipset: Arc<Mutex<Arc<Tipset>>>,
    /// The underlying provider
    pub api: Arc<T>,
    /// The minimum gas price needed for executing the transaction based on
    /// number of included blocks
    pub min_gas_price: BigInt,
    /// This is max number of messages in the pool.
    pub max_tx_pool_size: i64,
    // TODO
    pub network_name: String,
    /// Sender half to send messages to other components
    pub network_sender: flume::Sender<NetworkMessage>,
    /// A cache for BLS signature keyed by Cid
    pub bls_sig_cache: Arc<Mutex<LruCache<Cid, Signature>>>,
    /// A cache for BLS signature keyed by Cid
    pub sig_val_cache: Arc<Mutex<LruCache<Cid, ()>>>,
    /// A set of republished messages identified by their Cid
    pub republished: Arc<SyncRwLock<HashSet<Cid>>>,
    /// Acts as a signal to republish messages from the republished set of
    /// messages
    pub repub_trigger: flume::Sender<()>,
    // TODO look into adding a cap to `local_msgs`
    local_msgs: Arc<SyncRwLock<HashSet<SignedMessage>>>,
    /// Configurable parameters of the message pool
    pub config: MpoolConfig,
    /// Chain configuration
    pub chain_config: Arc<ChainConfig>,
}

impl<T> MessagePool<T>
where
    T: Provider + std::marker::Send + std::marker::Sync + 'static,
{
    /// Creates a new `MessagePool` instance.
    pub fn new(
        api: T,
        network_name: String,
        network_sender: flume::Sender<NetworkMessage>,
        config: MpoolConfig,
        chain_config: Arc<ChainConfig>,
        services: &mut JoinSet<anyhow::Result<()>>,
    ) -> Result<MessagePool<T>, Error>
    where
        T: Provider,
    {
        let local_addrs = Arc::new(SyncRwLock::new(Vec::new()));
        let pending = Arc::new(SyncRwLock::new(HashMap::new()));
        let tipset = Arc::new(Mutex::new(api.get_heaviest_tipset()));
        let bls_sig_cache = Arc::new(Mutex::new(LruCache::new(BLS_SIG_CACHE_SIZE)));
        let sig_val_cache = Arc::new(Mutex::new(LruCache::new(SIG_VAL_CACHE_SIZE)));
        let local_msgs = Arc::new(SyncRwLock::new(HashSet::new()));
        let republished = Arc::new(SyncRwLock::new(HashSet::new()));
        let block_delay = chain_config.block_delay_secs;

        let (repub_trigger, repub_trigger_rx) = flume::bounded::<()>(4);
        let mut mp = MessagePool {
            local_addrs,
            pending,
            cur_tipset: tipset,
            api: Arc::new(api),
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
            chain_config: Arc::clone(&chain_config),
        };

        mp.load_local()?;

        let mut subscriber = mp.api.subscribe_head_changes();

        let api = mp.api.clone();
        let bls_sig_cache = mp.bls_sig_cache.clone();
        let pending = mp.pending.clone();
        let republished = mp.republished.clone();

        let cur_tipset = mp.cur_tipset.clone();
        let repub_trigger = Arc::new(mp.repub_trigger.clone());

        // Reacts to new HeadChanges
        services.spawn(async move {
            loop {
                match subscriber.recv().await {
                    Ok(ts) => {
                        let (cur, rev, app) = match ts {
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
                            cur.as_ref(),
                            rev,
                            app,
                        )
                        .await
                        .context("Error changing head")?;
                    }
                    Err(RecvError::Lagged(e)) => {
                        warn!("Head change subscriber lagged: skipping {} events", e);
                    }
                    Err(RecvError::Closed) => {
                        break Ok(());
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
        let republish_interval = 10 * block_delay + chain_config.propagation_delay_secs;
        // Reacts to republishing requests
        services.spawn(async move {
            let mut repub_trigger_rx = repub_trigger_rx.stream();
            let mut interval = interval(Duration::from_secs(republish_interval));
            loop {
                tokio::select! {
                    _ = interval.tick() => (),
                    _ = repub_trigger_rx.next() => (),
                }
                if let Err(e) = republish_pending_messages(
                    api.as_ref(),
                    network_sender.as_ref(),
                    network_name.as_ref(),
                    pending.as_ref(),
                    cur_tipset.as_ref(),
                    republished.as_ref(),
                    local_addrs.as_ref(),
                    &chain_config,
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
    fn add_local(&self, m: SignedMessage) -> Result<(), Error> {
        self.local_addrs.write().push(m.from());
        self.local_msgs.write().insert(m);
        Ok(())
    }

    /// Push a signed message to the `MessagePool`. Additionally performs basic
    /// checks on the validity of a message.
    pub async fn push(&self, msg: SignedMessage) -> Result<Cid, Error> {
        self.check_message(&msg)?;
        let cid = msg.cid().map_err(|err| Error::Other(err.to_string()))?;
        let cur_ts = self.cur_tipset.lock().clone();
        let publish = self.add_tipset(msg.clone(), &cur_ts, true)?;
        let msg_ser = to_vec(&msg)?;
        self.add_local(msg)?;
        if publish {
            self.network_sender
                .send_async(NetworkMessage::PubsubMessage {
                    topic: Topic::new(format!("{}/{}", PUBSUB_MSG_STR, self.network_name)),
                    message: msg_ser,
                })
                .await
                .map_err(|_| Error::Other("Network receiver dropped".to_string()))?;
        }
        Ok(cid)
    }

    fn check_message(&self, msg: &SignedMessage) -> Result<(), Error> {
        if to_vec(msg)?.len() > 32 * 1024 {
            return Err(Error::MessageTooBig);
        }
        valid_for_block_inclusion(msg.message(), Gas::new(0), NEWEST_NETWORK_VERSION)?;
        if msg.value() > *crate::shim::econ::TOTAL_FILECOIN {
            return Err(Error::MessageValueTooHigh);
        }
        if msg.gas_fee_cap().atto() < &MINIMUM_BASE_FEE.into() {
            return Err(Error::GasFeeCapTooLow);
        }
        self.verify_msg_sig(msg)
    }

    /// This is a helper to push that will help to make sure that the message
    /// fits the parameters to be pushed to the `MessagePool`.
    pub fn add(&self, msg: SignedMessage) -> Result<(), Error> {
        self.check_message(&msg)?;

        let tip = self.cur_tipset.lock().clone();

        self.add_tipset(msg, &tip, false)?;
        Ok(())
    }

    /// Verify the message signature. first check if it has already been
    /// verified and put into cache. If it has not, then manually verify it
    /// then put it into cache for future use.
    fn verify_msg_sig(&self, msg: &SignedMessage) -> Result<(), Error> {
        let cid = msg.cid()?;

        if let Some(()) = self.sig_val_cache.lock().get(&cid) {
            return Ok(());
        }

        msg.verify().map_err(Error::Other)?;

        self.sig_val_cache.lock().put(cid, ());

        Ok(())
    }

    /// Verify the `state_sequence` and balance for the sender of the message
    /// given then call `add_locked` to finish adding the `signed_message`
    /// to pending.
    fn add_tipset(&self, msg: SignedMessage, cur_ts: &Tipset, local: bool) -> Result<bool, Error> {
        let sequence = self.get_state_sequence(&msg.from(), cur_ts)?;

        if sequence > msg.message().sequence {
            return Err(Error::SequenceTooLow);
        }

        let sender_actor = self.api.get_actor_after(&msg.message().from(), cur_ts)?;

        // This message can only be included in the next epoch and beyond, hence the +1.
        let nv = self.chain_config.network_version(cur_ts.epoch() + 1);
        if !is_valid_for_sending(nv, &sender_actor) {
            return Err(Error::Other(
                "Sender actor is not a valid top-level sender".to_owned(),
            ));
        }

        let publish = verify_msg_before_add(&msg, cur_ts, local, &self.chain_config)?;

        let balance = self.get_state_balance(&msg.from(), cur_ts)?;

        let msg_balance = msg.required_funds();
        if balance < msg_balance {
            return Err(Error::NotEnoughFunds);
        }
        self.add_helper(msg)?;
        Ok(publish)
    }

    /// Finish verifying signed message before adding it to the pending `mset`
    /// hash-map. If an entry in the hash-map does not yet exist, create a
    /// new `mset` that will correspond to the from message and push it to
    /// the pending hash-map.
    fn add_helper(&self, msg: SignedMessage) -> Result<(), Error> {
        let from = msg.from();
        let cur_ts = self.cur_tipset.lock().clone();
        add_helper(
            self.api.as_ref(),
            self.bls_sig_cache.as_ref(),
            self.pending.as_ref(),
            msg,
            self.get_state_sequence(&from, &cur_ts)?,
        )
    }

    /// Get the sequence for a given address, return Error if there is a failure
    /// to retrieve the respective sequence.
    pub fn get_sequence(&self, addr: &Address) -> Result<u64, Error> {
        let cur_ts = self.cur_tipset.lock().clone();

        let sequence = self.get_state_sequence(addr, &cur_ts)?;

        let pending = self.pending.read();

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

    /// Get the state of the sequence for a given address in `cur_ts`.
    fn get_state_sequence(&self, addr: &Address, cur_ts: &Tipset) -> Result<u64, Error> {
        let actor = self.api.get_actor_after(addr, cur_ts)?;
        Ok(actor.sequence)
    }

    /// Get the state balance for the actor that corresponds to the supplied
    /// address and tipset, if this actor does not exist, return an error.
    fn get_state_balance(&self, addr: &Address, ts: &Tipset) -> Result<TokenAmount, Error> {
        let actor = self.api.get_actor_after(addr, ts)?;
        Ok(TokenAmount::from(&actor.balance))
    }

    /// Return a tuple that contains a vector of all signed messages and the
    /// current tipset for self.
    pub fn pending(&self) -> Result<(Vec<SignedMessage>, Arc<Tipset>), Error> {
        let mut out: Vec<SignedMessage> = Vec::new();
        let pending = self.pending.read().clone();

        for (addr, _) in pending {
            out.append(
                self.pending_for(&addr)
                    .ok_or(Error::InvalidFromAddr)?
                    .as_mut(),
            )
        }

        let cur_ts = self.cur_tipset.lock().clone();

        Ok((out, cur_ts))
    }

    /// Return a Vector of signed messages for a given from address. This vector
    /// will be sorted by each `messsage`'s sequence. If no corresponding
    /// messages found, return None result type.
    pub fn pending_for(&self, a: &Address) -> Option<Vec<SignedMessage>> {
        let pending = self.pending.read();
        let mset = pending.get(a)?;
        if mset.msgs.is_empty() {
            return None;
        }
        let mut msg_vec = Vec::new();
        for (_, item) in mset.msgs.iter() {
            msg_vec.push(item.clone());
        }
        msg_vec.sort_by_key(|value| value.message().sequence);
        Some(msg_vec)
    }

    /// Return Vector of signed messages given a block header for self.
    pub fn messages_for_blocks(&self, blks: &[BlockHeader]) -> Result<Vec<SignedMessage>, Error> {
        let mut msg_vec: Vec<SignedMessage> = Vec::new();

        for block in blks {
            let (umsg, mut smsgs) = self.api.messages_for_block(block)?;

            msg_vec.append(smsgs.as_mut());
            for msg in umsg {
                let smsg = recover_sig(&mut self.bls_sig_cache.lock(), msg)?;
                msg_vec.push(smsg)
            }
        }
        Ok(msg_vec)
    }

    /// Loads local messages to the message pool to be applied.
    pub fn load_local(&mut self) -> Result<(), Error> {
        let mut local_msgs = self.local_msgs.write();
        for k in local_msgs.iter().cloned().collect::<Vec<SignedMessage>>() {
            self.add(k.clone()).unwrap_or_else(|err| {
                if err == Error::SequenceTooLow {
                    warn!("error adding message: {:?}", err);
                    local_msgs.remove(&k);
                }
            })
        }

        Ok(())
    }

    #[cfg(test)]
    pub fn get_config(&self) -> &MpoolConfig {
        &self.config
    }

    #[cfg(test)]
    pub fn set_config<DB: Store>(&mut self, db: &DB, cfg: MpoolConfig) -> Result<(), Error> {
        cfg.save_config(db)
            .map_err(|e| Error::Other(e.to_string()))?;
        self.config = cfg;
        Ok(())
    }

    /// Select messages that can be included in a block built on a given base
    /// tipset.
    pub fn select_messages_for_block(&self, base: &Tipset) -> Result<Vec<SignedMessage>, Error> {
        // Take a snapshot of the pending messages.
        let pending: HashMap<Address, HashMap<u64, SignedMessage>> = {
            let pending = self.pending.read();
            pending
                .iter()
                .filter_map(|(actor, mset)| {
                    if mset.msgs.is_empty() {
                        None
                    } else {
                        Some((*actor, mset.msgs.clone()))
                    }
                })
                .collect()
        };

        select_messages_for_block(self.api.as_ref(), self.chain_config.as_ref(), base, pending)
    }
}

// Helpers for MessagePool

/// Finish verifying signed message before adding it to the pending `mset`
/// hash-map. If an entry in the hash-map does not yet exist, create a new
/// `mset` that will correspond to the from message and push it to the pending
/// hash-map.
pub(in crate::message_pool) fn add_helper<T>(
    api: &T,
    bls_sig_cache: &Mutex<LruCache<Cid, Signature>>,
    pending: &SyncRwLock<HashMap<Address, MsgSet>>,
    msg: SignedMessage,
    sequence: u64,
) -> Result<(), Error>
where
    T: Provider,
{
    if msg.signature().signature_type() == SignatureType::Bls {
        bls_sig_cache
            .lock()
            .put(msg.cid()?, msg.signature().clone());
    }

    if msg.message().gas_limit > 100_000_000 {
        return Err(Error::Other(
            "given message has too high of a gas limit".to_string(),
        ));
    }

    api.put_message(&ChainMessage::Signed(msg.clone()))?;
    api.put_message(&ChainMessage::Unsigned(msg.message().clone()))?;

    let mut pending = pending.write();
    let msett = pending.get_mut(&msg.from());
    match msett {
        Some(mset) => mset.add(msg)?,
        None => {
            let mut mset = MsgSet::new(sequence);
            let from = msg.from();
            mset.add(msg)?;
            pending.insert(from, mset);
        }
    }

    Ok(())
}

fn verify_msg_before_add(
    m: &SignedMessage,
    cur_ts: &Tipset,
    local: bool,
    chain_config: &ChainConfig,
) -> Result<bool, Error> {
    let epoch = cur_ts.epoch();
    let min_gas = price_list_by_network_version(chain_config.network_version(epoch))
        .on_chain_message(to_vec(m)?.len());
    valid_for_block_inclusion(m.message(), min_gas.total(), NEWEST_NETWORK_VERSION)?;
    if !cur_ts.blocks().is_empty() {
        let base_fee = cur_ts.blocks()[0].parent_base_fee();
        let base_fee_lower_bound =
            get_base_fee_lower_bound(base_fee, BASE_FEE_LOWER_BOUND_FACTOR_CONSERVATIVE);
        if m.gas_fee_cap() < base_fee_lower_bound {
            if local {
                warn!("local message will not be immediately published because GasFeeCap doesn't meet the lower bound for inclusion in the next 20 blocks (GasFeeCap: {}, baseFeeLowerBound: {})",m.gas_fee_cap(), base_fee_lower_bound);
                return Ok(false);
            }
            return Err(Error::SoftValidationFailure(format!("GasFeeCap doesn't meet base fee lower bound for inclusion in the next 20 blocks (GasFeeCap: {}, baseFeeLowerBound:{})",
                m.gas_fee_cap(), base_fee_lower_bound)));
        }
    }
    Ok(local)
}

/// Remove a message from pending given the from address and sequence.
pub fn remove(
    from: &Address,
    pending: &SyncRwLock<HashMap<Address, MsgSet>>,
    sequence: u64,
    applied: bool,
) -> Result<(), Error> {
    let mut pending = pending.write();
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
