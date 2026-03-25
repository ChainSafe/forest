// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

// Contains the implementation of Message Pool component.
// The Message Pool is the component of forest that handles pending messages for
// inclusion in the chain. Messages are added either directly for locally
// published messages or through pubsub propagation.

use std::{num::NonZeroUsize, sync::Arc, time::Duration};

use crate::blocks::{CachingBlockHeader, Tipset, TipsetKey};
use crate::chain::{HeadChanges, MINIMUM_BASE_FEE};
#[cfg(test)]
use crate::db::SettingsStore;
use crate::eth::is_valid_eth_tx_for_sending;
use crate::libp2p::{NetworkMessage, PUBSUB_MSG_STR, Topic};
use crate::message::{ChainMessage, MessageRead as _, SignedMessage, valid_for_block_inclusion};
use crate::networks::{ChainConfig, NEWEST_NETWORK_VERSION};
use crate::rpc::eth::types::EthAddress;
use crate::shim::{
    address::{Address, Protocol},
    crypto::{Signature, SignatureType},
    econ::TokenAmount,
    gas::{Gas, price_list_by_network_version},
};
use crate::state_manager::utils::is_valid_for_sending;
use crate::utils::cache::SizeTrackingLruCache;
use crate::utils::get_size::CidWrapper;
use ahash::{HashMap, HashMapExt, HashSet, HashSetExt};
use anyhow::Context as _;
use cid::Cid;
use futures::StreamExt;
use fvm_ipld_encoding::to_vec;
use get_size2::GetSize;
use itertools::Itertools;
use nonzero_ext::nonzero;
use parking_lot::RwLock as SyncRwLock;
use tokio::{sync::broadcast::error::RecvError, task::JoinSet, time::interval};
use tracing::warn;

use crate::message_pool::{
    config::MpoolConfig,
    errors::Error,
    head_change, metrics,
    msgpool::{
        BASE_FEE_LOWER_BOUND_FACTOR_CONSERVATIVE, RBF_DENOM, RBF_NUM, recover_sig,
        republish_pending_messages,
    },
    provider::Provider,
    utils::get_base_fee_lower_bound,
};

// LruCache sizes have been taken from the lotus implementation
const BLS_SIG_CACHE_SIZE: NonZeroUsize = nonzero!(40000usize);
const SIG_VAL_CACHE_SIZE: NonZeroUsize = nonzero!(32000usize);
const KEY_CACHE_SIZE: NonZeroUsize = nonzero!(1_048_576usize);
const STATE_NONCE_CACHE_SIZE: NonZeroUsize = nonzero!(32768usize);

#[derive(Clone, Debug, Hash, PartialEq, Eq, GetSize)]
pub(crate) struct StateNonceCacheKey {
    tipset_key: TipsetKey,
    addr: Address,
}

pub const MAX_ACTOR_PENDING_MESSAGES: u64 = 1000;
pub const MAX_UNTRUSTED_ACTOR_PENDING_MESSAGES: u64 = 10;
const MAX_NONCE_GAP: u64 = 4;
/// Maximum size of a serialized message in bytes. This is an anti-DOS measure to prevent
/// large messages from being added to the message pool.
const MAX_MESSAGE_SIZE: usize = 64 << 10; // 64 KiB

/// Trust policy for whether a message is from a trusted or untrusted source.
/// Untrusted sources are subject to stricter limits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrustPolicy {
    Trusted,
    Untrusted,
}

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
    /// Use this method when pushing a message coming from trusted sources.
    pub fn add_trusted<T>(&mut self, api: &T, m: SignedMessage, strict: bool) -> Result<(), Error>
    where
        T: Provider,
    {
        self.add(api, m, strict, true)
    }

    /// Add a signed message to the `MsgSet`. Increase `next_sequence` if the
    /// message has a sequence greater than any existing message sequence.
    /// Use this method when pushing a message coming from untrusted sources.
    pub fn add_untrusted<T>(&mut self, api: &T, m: SignedMessage, strict: bool) -> Result<(), Error>
    where
        T: Provider,
    {
        self.add(api, m, strict, false)
    }

    pub(in crate::message_pool) fn add<T>(
        &mut self,
        api: &T,
        m: SignedMessage,
        strict: bool,
        trusted: bool,
    ) -> Result<(), Error>
    where
        T: Provider,
    {
        let max_nonce_gap: u64 = if trusted { MAX_NONCE_GAP } else { 0 };
        let max_actor_pending_messages = if trusted {
            api.max_actor_pending_messages()
        } else {
            api.max_untrusted_actor_pending_messages()
        };

        let mut next_nonce = self.next_sequence;
        let nonce_gap = if m.sequence() == next_nonce {
            next_nonce += 1;
            while self.msgs.contains_key(&next_nonce) {
                next_nonce += 1;
            }
            false
        } else if strict && m.sequence() > next_nonce + max_nonce_gap {
            tracing::debug!(
                nonce = m.sequence(),
                next_nonce,
                "message nonce has too big a gap from expected nonce"
            );
            return Err(Error::NonceGap);
        } else if m.sequence() > next_nonce {
            true
        } else {
            false
        };

        let has_existing = if let Some(exms) = self.msgs.get(&m.sequence()) {
            if strict && nonce_gap {
                tracing::debug!(
                    nonce = m.sequence(),
                    next_nonce,
                    "rejecting replace by fee because of nonce gap"
                );
                return Err(Error::NonceGap);
            }
            if m.cid() != exms.cid() {
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
            true
        } else {
            false
        };

        // Only check the limit when adding a new message, not when replacing an existing one (RBF)
        if !has_existing && self.msgs.len() as u64 >= max_actor_pending_messages {
            return Err(Error::TooManyPendingMessages(
                m.message.from().to_string(),
                trusted,
            ));
        }

        if strict && nonce_gap {
            tracing::debug!(
                from = %m.from(),
                nonce = m.sequence(),
                next_nonce,
                "adding nonce-gapped message"
            );
        }

        self.next_sequence = next_nonce;
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
                while self.msgs.contains_key(&self.next_sequence) {
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
    /// A map of pending messages where the key is the resolved key address
    pub pending: Arc<SyncRwLock<HashMap<Address, MsgSet>>>,
    /// The current tipset (a set of blocks)
    pub cur_tipset: Arc<SyncRwLock<Tipset>>,
    /// The underlying provider
    pub api: Arc<T>,
    /// Sender half to send messages to other components
    pub network_sender: flume::Sender<NetworkMessage>,
    /// A cache for BLS signature keyed by Cid
    pub bls_sig_cache: Arc<SizeTrackingLruCache<CidWrapper, Signature>>,
    /// A cache for BLS signature keyed by Cid
    pub sig_val_cache: Arc<SizeTrackingLruCache<CidWrapper, ()>>,
    /// Cache for ID address to key address resolution.
    pub key_cache: Arc<SizeTrackingLruCache<Address, Address>>,
    /// Cache for state nonce lookups keyed by (TipsetKey, Address).
    pub state_nonce_cache: Arc<SizeTrackingLruCache<StateNonceCacheKey, u64>>,
    /// A set of republished messages identified by their Cid
    pub republished: Arc<SyncRwLock<HashSet<Cid>>>,
    /// Acts as a signal to republish messages from the republished set of
    /// messages
    pub repub_trigger: flume::Sender<()>,
    local_msgs: Arc<SyncRwLock<HashSet<SignedMessage>>>,
    /// Configurable parameters of the message pool
    pub config: MpoolConfig,
    /// Chain configuration
    pub chain_config: Arc<ChainConfig>,
}

/// Resolve an address to its key form, checking the cache first.
/// Non-ID addresses are returned unchanged.
pub(in crate::message_pool) fn resolve_to_key<T: Provider>(
    api: &T,
    key_cache: &SizeTrackingLruCache<Address, Address>,
    addr: &Address,
    cur_ts: &Tipset,
) -> Result<Address, Error> {
    if addr.protocol() != Protocol::ID {
        return Ok(*addr);
    }
    if let Some(resolved) = key_cache.get_cloned(addr) {
        return Ok(resolved);
    }
    let resolved = api.resolve_to_key(addr, cur_ts)?;
    key_cache.push(*addr, resolved);
    Ok(resolved)
}

/// Get the state nonce for an address, accounting for messages already included in `cur_ts`.
pub(in crate::message_pool) fn get_state_sequence<T: Provider>(
    api: &T,
    key_cache: &SizeTrackingLruCache<Address, Address>,
    state_nonce_cache: &SizeTrackingLruCache<StateNonceCacheKey, u64>,
    addr: &Address,
    cur_ts: &Tipset,
) -> Result<u64, Error> {
    let nk = StateNonceCacheKey {
        tipset_key: cur_ts.key().clone(),
        addr: *addr,
    };

    if let Some(cached) = state_nonce_cache.get_cloned(&nk) {
        return Ok(cached);
    }

    let actor = api.get_actor_after(addr, cur_ts)?;
    let mut next_nonce = actor.sequence;

    let resolved = resolve_to_key(api, key_cache, addr, cur_ts)?;
    let messages = api.messages_for_tipset(cur_ts)?;
    for msg in &messages {
        let from = resolve_to_key(api, key_cache, &msg.from(), cur_ts).unwrap_or(msg.from());
        if from == resolved {
            let n = msg.sequence() + 1;
            if n > next_nonce {
                next_nonce = n;
            }
        }
    }

    state_nonce_cache.push(nk, next_nonce);
    Ok(next_nonce)
}

impl<T> MessagePool<T>
where
    T: Provider,
{
    /// Gets the current tipset
    pub fn current_tipset(&self) -> Tipset {
        self.cur_tipset.read().clone()
    }

    pub fn resolve_to_key(&self, addr: &Address, cur_ts: &Tipset) -> Result<Address, Error> {
        resolve_to_key(self.api.as_ref(), self.key_cache.as_ref(), addr, cur_ts)
    }

    /// Add a signed message to the pool and its address.
    fn add_local(&self, m: SignedMessage) -> Result<(), Error> {
        let cur_ts = self.current_tipset();
        let resolved = self.resolve_to_key(&m.from(), &cur_ts)?;
        self.local_addrs.write().push(resolved);
        self.local_msgs.write().insert(m);
        Ok(())
    }

    /// Push a signed message to the `MessagePool`. Additionally performs basic
    /// checks on the validity of a message.
    pub async fn push_internal(
        &self,
        msg: SignedMessage,
        trust_policy: TrustPolicy,
    ) -> Result<Cid, Error> {
        self.check_message(&msg)?;
        let cid = msg.cid();
        let cur_ts = self.current_tipset();
        let publish = self.add_tipset(msg.clone(), &cur_ts, true, trust_policy)?;
        let msg_ser = to_vec(&msg)?;
        let network_name = self.chain_config.network.genesis_name();
        self.add_local(msg)?;
        if publish {
            self.network_sender
                .send_async(NetworkMessage::PubsubMessage {
                    topic: Topic::new(format!("{PUBSUB_MSG_STR}/{network_name}")),
                    message: msg_ser,
                })
                .await
                .map_err(|_| Error::Other("Network receiver dropped".to_string()))?;
        }
        Ok(cid)
    }

    /// Push a signed message to the `MessagePool` from an trusted source.
    pub async fn push(&self, msg: SignedMessage) -> Result<Cid, Error> {
        self.push_internal(msg, TrustPolicy::Trusted).await
    }

    /// Push a signed message to the `MessagePool` from an untrusted source.
    pub async fn push_untrusted(&self, msg: SignedMessage) -> Result<Cid, Error> {
        self.push_internal(msg, TrustPolicy::Untrusted).await
    }

    fn check_message(&self, msg: &SignedMessage) -> Result<(), Error> {
        if to_vec(msg)?.len() > MAX_MESSAGE_SIZE {
            return Err(Error::MessageTooBig);
        }
        let to = msg.message().to();
        if to.protocol() == Protocol::Delegated {
            EthAddress::from_filecoin_address(&to).context(format!(
                "message recipient {to} is a delegated address but not a valid Eth Address"
            ))?;
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
        let ts = self.current_tipset();
        self.add_tipset(msg, &ts, false, TrustPolicy::Trusted)?;
        Ok(())
    }

    /// Verify the message signature. first check if it has already been
    /// verified and put into cache. If it has not, then manually verify it
    /// then put it into cache for future use.
    fn verify_msg_sig(&self, msg: &SignedMessage) -> Result<(), Error> {
        let cid = msg.cid();

        if let Some(()) = self.sig_val_cache.get_cloned(&(cid).into()) {
            return Ok(());
        }

        msg.verify(self.chain_config.eth_chain_id)
            .map_err(|e| Error::Other(e.to_string()))?;

        self.sig_val_cache.push(cid.into(), ());

        Ok(())
    }

    /// Verify the `state_sequence` and balance for the sender of the message
    /// given then call `add_locked` to finish adding the `signed_message`
    /// to pending.
    fn add_tipset(
        &self,
        msg: SignedMessage,
        cur_ts: &Tipset,
        local: bool,
        trust_policy: TrustPolicy,
    ) -> Result<bool, Error> {
        let sequence = self.get_state_sequence(&msg.from(), cur_ts)?;

        if sequence > msg.message().sequence {
            return Err(Error::SequenceTooLow);
        }

        let sender_actor = self.api.get_actor_after(&msg.message().from(), cur_ts)?;

        // This message can only be included in the next epoch and beyond, hence the +1.
        let nv = self.chain_config.network_version(cur_ts.epoch() + 1);
        let eth_chain_id = self.chain_config.eth_chain_id;
        if msg.signature().signature_type() == SignatureType::Delegated
            && !is_valid_eth_tx_for_sending(eth_chain_id, nv, &msg)
        {
            return Err(Error::Other(
                "Invalid Ethereum message for the current network version".to_owned(),
            ));
        }
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
        self.add_helper(msg, trust_policy, !local)?;
        Ok(publish)
    }

    /// Finish verifying signed message before adding it to the pending `mset`
    /// hash-map. If an entry in the hash-map does not yet exist, create a
    /// new `mset` that will correspond to the from message and push it to
    /// the pending hash-map.
    fn add_helper(
        &self,
        msg: SignedMessage,
        trust_policy: TrustPolicy,
        strict: bool,
    ) -> Result<(), Error> {
        let from = msg.from();
        let cur_ts = self.current_tipset();
        add_helper(
            self.api.as_ref(),
            self.bls_sig_cache.as_ref(),
            self.pending.as_ref(),
            self.key_cache.as_ref(),
            &cur_ts,
            msg,
            self.get_state_sequence(&from, &cur_ts)?,
            trust_policy,
            strict,
        )
    }

    /// Get the sequence for a given address, return Error if there is a failure
    /// to retrieve the respective sequence.
    pub fn get_sequence(&self, addr: &Address) -> Result<u64, Error> {
        let cur_ts = self.current_tipset();

        let sequence = self.get_state_sequence(addr, &cur_ts)?;

        let resolved = self.resolve_to_key(addr, &cur_ts)?;
        let pending = self.pending.read();

        let msgset = pending.get(&resolved);
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
        get_state_sequence(
            self.api.as_ref(),
            self.key_cache.as_ref(),
            self.state_nonce_cache.as_ref(),
            addr,
            cur_ts,
        )
    }

    /// Get the state balance for the actor that corresponds to the supplied
    /// address and tipset, if this actor does not exist, return an error.
    fn get_state_balance(&self, addr: &Address, ts: &Tipset) -> Result<TokenAmount, Error> {
        let actor = self.api.get_actor_after(addr, ts)?;
        Ok(TokenAmount::from(&actor.balance))
    }

    /// Return a tuple that contains a vector of all signed messages and the
    /// current tipset for self.
    pub fn pending(&self) -> Result<(Vec<SignedMessage>, Tipset), Error> {
        let mut out: Vec<SignedMessage> = Vec::new();
        let pending = self.pending.read().clone();

        for (addr, _) in pending {
            out.append(
                self.pending_for(&addr)
                    .ok_or(Error::InvalidFromAddr)?
                    .as_mut(),
            )
        }

        let cur_ts = self.current_tipset();

        Ok((out, cur_ts))
    }

    /// Return a Vector of signed messages for a given from address. This vector
    /// will be sorted by each `message`'s sequence. If no corresponding
    /// messages found, return None result type.
    pub fn pending_for(&self, a: &Address) -> Option<Vec<SignedMessage>> {
        let cur_ts = self.current_tipset();
        let resolved = self.resolve_to_key(a, &cur_ts).ok()?;
        let pending = self.pending.read();
        let mset = pending.get(&resolved)?;
        if mset.msgs.is_empty() {
            return None;
        }

        Some(
            mset.msgs
                .values()
                .cloned()
                .sorted_by_key(|v| v.message().sequence)
                .collect(),
        )
    }

    /// Return Vector of signed messages given a block header for self.
    pub fn messages_for_blocks<'a>(
        &self,
        blks: impl Iterator<Item = &'a CachingBlockHeader>,
    ) -> Result<Vec<SignedMessage>, Error> {
        let mut msg_vec: Vec<SignedMessage> = Vec::new();

        for block in blks {
            let (umsg, mut smsgs) = self.api.messages_for_block(block)?;

            msg_vec.append(smsgs.as_mut());
            for msg in umsg {
                let smsg = recover_sig(self.bls_sig_cache.as_ref(), msg)?;
                msg_vec.push(smsg)
            }
        }
        Ok(msg_vec)
    }

    /// Loads local messages to the message pool to be applied.
    pub fn load_local(&mut self) -> Result<(), Error> {
        let mut local_msgs = self.local_msgs.write();
        for k in local_msgs.iter().cloned().collect_vec() {
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
    pub fn set_config<DB: SettingsStore>(
        &mut self,
        db: &DB,
        cfg: MpoolConfig,
    ) -> Result<(), Error> {
        cfg.save_config(db)
            .map_err(|e| Error::Other(e.to_string()))?;
        self.config = cfg;
        Ok(())
    }
}

impl<T> MessagePool<T>
where
    T: Provider + Send + Sync + 'static,
{
    /// Creates a new `MessagePool` instance.
    pub fn new(
        api: T,
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
        let tipset = Arc::new(SyncRwLock::new(api.get_heaviest_tipset()));
        let bls_sig_cache = Arc::new(SizeTrackingLruCache::new_with_metrics(
            "bls_sig".into(),
            BLS_SIG_CACHE_SIZE,
        ));
        let sig_val_cache = Arc::new(SizeTrackingLruCache::new_with_metrics(
            "sig_val".into(),
            SIG_VAL_CACHE_SIZE,
        ));
        let key_cache = Arc::new(SizeTrackingLruCache::new_with_metrics(
            "mpool_key".into(),
            KEY_CACHE_SIZE,
        ));
        let state_nonce_cache = Arc::new(SizeTrackingLruCache::new_with_metrics(
            "state_nonce".into(),
            STATE_NONCE_CACHE_SIZE,
        ));
        let local_msgs = Arc::new(SyncRwLock::new(HashSet::new()));
        let republished = Arc::new(SyncRwLock::new(HashSet::new()));
        let block_delay = chain_config.block_delay_secs;

        let (repub_trigger, repub_trigger_rx) = flume::bounded::<()>(4);
        let mut mp = MessagePool {
            local_addrs,
            pending,
            cur_tipset: tipset,
            api: Arc::new(api),
            bls_sig_cache,
            sig_val_cache,
            key_cache,
            state_nonce_cache,
            local_msgs,
            republished,
            config,
            network_sender,
            repub_trigger,
            chain_config: Arc::clone(&chain_config),
        };

        mp.load_local()?;

        let mut head_changes_rx = mp.api.subscribe_head_changes();

        let api = mp.api.clone();
        let bls_sig_cache = mp.bls_sig_cache.clone();
        let pending = mp.pending.clone();
        let republished = mp.republished.clone();
        let key_cache = mp.key_cache.clone();
        let state_nonce_cache = mp.state_nonce_cache.clone();

        let current_ts = mp.cur_tipset.clone();
        let repub_trigger = mp.repub_trigger.clone();

        // Reacts to new HeadChanges
        services.spawn(async move {
            loop {
                match head_changes_rx.recv().await {
                    Ok(HeadChanges { reverts, applies }) => {
                        if let Err(e) = head_change(
                            api.as_ref(),
                            bls_sig_cache.as_ref(),
                            repub_trigger.clone(),
                            republished.as_ref(),
                            pending.as_ref(),
                            &current_ts,
                            key_cache.as_ref(),
                            state_nonce_cache.as_ref(),
                            reverts,
                            applies,
                        )
                        .await
                        {
                            tracing::warn!("Error changing head: {e}");
                        }
                    }
                    Err(RecvError::Lagged(e)) => {
                        warn!("Head change subscriber lagged: skipping {e} events");
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
        let key_cache = mp.key_cache.clone();
        let network_sender = Arc::new(mp.network_sender.clone());
        let republish_interval = u64::from(10 * block_delay + chain_config.propagation_delay_secs);
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
                    pending.as_ref(),
                    cur_tipset.as_ref(),
                    republished.as_ref(),
                    local_addrs.as_ref(),
                    key_cache.as_ref(),
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
}

// Helpers for MessagePool

/// Finish verifying signed message before adding it to the pending `mset`
/// hash-map. If an entry in the hash-map does not yet exist, create a new
/// `mset` that will correspond to the from message and push it to the pending
/// hash-map.
#[allow(clippy::too_many_arguments)]
pub(in crate::message_pool) fn add_helper<T>(
    api: &T,
    bls_sig_cache: &SizeTrackingLruCache<CidWrapper, Signature>,
    pending: &SyncRwLock<HashMap<Address, MsgSet>>,
    key_cache: &SizeTrackingLruCache<Address, Address>,
    cur_ts: &Tipset,
    msg: SignedMessage,
    sequence: u64,
    trust_policy: TrustPolicy,
    strict: bool,
) -> Result<(), Error>
where
    T: Provider,
{
    if msg.signature().signature_type() == SignatureType::Bls {
        bls_sig_cache.push(msg.cid().into(), msg.signature().clone());
    }

    api.put_message(&ChainMessage::Signed(msg.clone().into()))?;
    api.put_message(&ChainMessage::Unsigned(msg.message().clone().into()))?;

    let resolved_from = resolve_to_key(api, key_cache, &msg.from(), cur_ts)?;
    let mut pending = pending.write();
    let mset = pending
        .entry(resolved_from)
        .or_insert_with(|| MsgSet::new(sequence));
    match trust_policy {
        TrustPolicy::Trusted => mset.add_trusted(api, msg, strict)?,
        TrustPolicy::Untrusted => mset.add_untrusted(api, msg, strict)?,
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
        .on_chain_message(m.chain_length()?);
    valid_for_block_inclusion(m.message(), min_gas.total(), NEWEST_NETWORK_VERSION)?;
    if !cur_ts.block_headers().is_empty() {
        let base_fee = &cur_ts.block_headers().first().parent_base_fee;
        let base_fee_lower_bound =
            get_base_fee_lower_bound(base_fee, BASE_FEE_LOWER_BOUND_FACTOR_CONSERVATIVE);
        if m.gas_fee_cap() < base_fee_lower_bound {
            if local {
                warn!(
                    "local message will not be immediately published because GasFeeCap doesn't meet the lower bound for inclusion in the next 20 blocks (GasFeeCap: {}, baseFeeLowerBound: {})",
                    m.gas_fee_cap(),
                    base_fee_lower_bound
                );
                return Ok(false);
            }
            return Err(Error::SoftValidationFailure(format!(
                "GasFeeCap doesn't meet base fee lower bound for inclusion in the next 20 blocks (GasFeeCap: {}, baseFeeLowerBound:{})",
                m.gas_fee_cap(),
                base_fee_lower_bound
            )));
        }
    }
    Ok(local)
}

/// Remove a message from pending given the from address and sequence.
/// The `from` address should already be resolved to its key form.
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

#[cfg(test)]
mod tests {
    use crate::message_pool::test_provider::TestApi;
    use crate::shim::econ::TokenAmount;

    use super::*;
    use crate::shim::message::Message as ShimMessage;

    fn make_smsg(from: Address, seq: u64, premium: u64) -> SignedMessage {
        SignedMessage::mock_bls_signed_message(ShimMessage {
            from: from.into(),
            sequence: seq,
            gas_premium: TokenAmount::from_atto(premium),
            gas_limit: 1_000_000,
            ..ShimMessage::default()
        })
    }

    // Regression test for https://github.com/ChainSafe/forest/pull/6118 which fixed a bogus 100M
    // gas limit. There are no limits on a single message.
    #[test]
    fn add_helper_message_gas_limit_test() {
        let api = TestApi::default();
        let bls_sig_cache = SizeTrackingLruCache::new_mocked();
        let key_cache = SizeTrackingLruCache::new_mocked();
        let pending = SyncRwLock::new(HashMap::new());
        let cur_ts = api.get_heaviest_tipset();
        let message = ShimMessage {
            gas_limit: 666_666_666,
            ..ShimMessage::default()
        };
        let msg = SignedMessage::mock_bls_signed_message(message);
        let sequence = msg.message().sequence;
        let res = add_helper(
            &api,
            &bls_sig_cache,
            &pending,
            &key_cache,
            &cur_ts,
            msg,
            sequence,
            TrustPolicy::Trusted,
            false,
        );
        assert!(res.is_ok());
    }

    // Test that RBF (Replace By Fee) is allowed even when at max_actor_pending_messages capacity
    // This matches Lotus behavior where the check is: https://github.com/filecoin-project/lotus/blob/5f32d00550ddd2f2d0f9abe97dbae07615f18547/chain/messagepool/messagepool.go#L296-L299
    #[test]
    fn test_rbf_at_capacity() {
        let api = TestApi::with_max_actor_pending_messages(10);
        let mut mset = MsgSet::new(0);

        // Fill up to capacity (10 messages)
        for i in 0..10 {
            let res = mset.add_trusted(&api, make_smsg(Address::default(), i, 100), false);
            assert!(res.is_ok(), "Failed to add message {}: {:?}", i, res);
        }

        // Should reject adding a NEW message (sequence 10) when at capacity
        let res = mset.add_trusted(&api, make_smsg(Address::default(), 10, 100), false);
        assert!(matches!(res, Err(Error::TooManyPendingMessages(_, _))));

        // Should ALLOW replacing an existing message (RBF) even when at capacity
        // Replace message with sequence 5 with higher gas premium
        let res = mset.add_trusted(&api, make_smsg(Address::default(), 5, 200), false);
        assert!(res.is_ok(), "RBF should be allowed at capacity: {:?}", res);
    }

    #[test]
    fn test_resolve_to_key_returns_non_id_unchanged() {
        let api = TestApi::default();
        let key_cache = SizeTrackingLruCache::new_mocked();
        let ts = api.get_heaviest_tipset();

        let bls_addr = Address::new_bls(&[1u8; 48]).unwrap();
        let result = resolve_to_key(&api, &key_cache, &bls_addr, &ts).unwrap();
        assert_eq!(result, bls_addr);
        assert_eq!(
            key_cache.len(),
            0,
            "cache should not be populated for non-ID addresses"
        );
    }

    #[test]
    fn test_resolve_to_key_resolves_id_and_caches() {
        let api = TestApi::default();
        let key_cache = SizeTrackingLruCache::new_mocked();
        let ts = api.get_heaviest_tipset();

        let id_addr = Address::new_id(100);
        let key_addr = Address::new_bls(&[5u8; 48]).unwrap();
        api.set_key_address_mapping(&id_addr, &key_addr);

        let result = resolve_to_key(&api, &key_cache, &id_addr, &ts).unwrap();
        assert_eq!(result, key_addr);
        assert_eq!(
            key_cache.len(),
            1,
            "cache should have one entry after resolution"
        );

        // Second call should hit the cache (no API call needed)
        let result2 = resolve_to_key(&api, &key_cache, &id_addr, &ts).unwrap();
        assert_eq!(result2, key_addr);
    }

    #[test]
    fn test_add_helper_keys_pending_by_resolved_address() {
        let api = TestApi::default();
        let bls_sig_cache = SizeTrackingLruCache::new_mocked();
        let key_cache = SizeTrackingLruCache::new_mocked();
        let pending = SyncRwLock::new(HashMap::new());
        let cur_ts = api.get_heaviest_tipset();

        let id_addr = Address::new_id(200);
        let key_addr = Address::new_bls(&[7u8; 48]).unwrap();
        api.set_key_address_mapping(&id_addr, &key_addr);
        api.set_state_sequence(&key_addr, 0);

        let message = ShimMessage {
            from: id_addr.into(),
            gas_limit: 1_000_000,
            ..ShimMessage::default()
        };
        let msg = SignedMessage::mock_bls_signed_message(message);

        add_helper(
            &api,
            &bls_sig_cache,
            &pending,
            &key_cache,
            &cur_ts,
            msg,
            0,
            TrustPolicy::Trusted,
            false,
        )
        .unwrap();

        let pending_read = pending.read();
        assert!(
            pending_read.get(&key_addr).is_some(),
            "pending should be keyed by the resolved key address"
        );
        assert!(
            pending_read.get(&id_addr).is_none(),
            "pending should NOT have an entry under the raw ID address"
        );
    }

    #[test]
    fn test_get_sequence_works_with_both_address_forms() {
        use crate::message_pool::provider::Provider;

        let api = TestApi::default();
        let bls_sig_cache = SizeTrackingLruCache::new_mocked();
        let key_cache = SizeTrackingLruCache::new_mocked();
        let pending = SyncRwLock::new(HashMap::new());
        let cur_ts = api.get_heaviest_tipset();

        let id_addr = Address::new_id(300);
        let key_addr = Address::new_bls(&[9u8; 48]).unwrap();
        api.set_key_address_mapping(&id_addr, &key_addr);
        api.set_state_sequence(&key_addr, 0);

        // Add two messages from the ID address
        for seq in 0..2 {
            let message = ShimMessage {
                from: id_addr.into(),
                sequence: seq,
                gas_limit: 1_000_000,
                ..ShimMessage::default()
            };
            let msg = SignedMessage::mock_bls_signed_message(message);
            add_helper(
                &api,
                &bls_sig_cache,
                &pending,
                &key_cache,
                &cur_ts,
                msg,
                0,
                TrustPolicy::Trusted,
                false,
            )
            .unwrap();
        }

        let state_seq = api.get_actor_after(&id_addr, &cur_ts).unwrap().sequence;
        let resolved_for_id = resolve_to_key(&api, &key_cache, &id_addr, &cur_ts).unwrap();
        let resolved_for_key = resolve_to_key(&api, &key_cache, &key_addr, &cur_ts).unwrap();
        assert_eq!(resolved_for_id, resolved_for_key);

        let mset = pending.read();
        let next_seq = mset.get(&resolved_for_id).unwrap().next_sequence;
        let expected = std::cmp::max(state_seq, next_seq);
        assert_eq!(expected, 2, "should reflect both pending messages");
    }

    #[test]
    fn test_gap_filling_advances_next_sequence() {
        let api = TestApi::default();
        let mut mset = MsgSet::new(0);

        mset.add_trusted(&api, make_smsg(Address::default(), 0, 100), false)
            .unwrap();
        assert_eq!(mset.next_sequence, 1);

        mset.add_trusted(&api, make_smsg(Address::default(), 2, 100), false)
            .unwrap();
        assert_eq!(mset.next_sequence, 1, "gap at 1, so next_sequence stays");

        mset.add_trusted(&api, make_smsg(Address::default(), 1, 100), false)
            .unwrap();
        assert_eq!(
            mset.next_sequence, 3,
            "filling the gap should advance past all consecutive messages"
        );
    }

    #[test]
    fn test_trusted_allows_any_nonce_gap() {
        let api = TestApi::default();
        let mut mset = MsgSet::new(0);

        mset.add_trusted(&api, make_smsg(Address::default(), 0, 100), false)
            .unwrap();
        let res = mset.add_trusted(&api, make_smsg(Address::default(), 10, 100), false);
        assert!(
            res.is_ok(),
            "trusted adds skip nonce gap enforcement (strict=false)"
        );
    }

    #[test]
    fn test_strict_allows_small_nonce_gap() {
        let api = TestApi::default();
        let mut mset = MsgSet::new(0);

        // strict=true, trusted=true -> max_nonce_gap=4 (gossipsub path)
        mset.add(&api, make_smsg(Address::default(), 0, 100), true, true)
            .unwrap();
        let res = mset.add(&api, make_smsg(Address::default(), 3, 100), true, true);
        assert!(
            res.is_ok(),
            "strict+trusted: gap of 2 (within MAX_NONCE_GAP=4) should succeed"
        );
    }

    #[test]
    fn test_strict_rejects_large_nonce_gap() {
        let api = TestApi::default();
        let mut mset = MsgSet::new(0);

        // strict=true, trusted=true -> max_nonce_gap=4
        mset.add(&api, make_smsg(Address::default(), 0, 100), true, true)
            .unwrap();
        let res = mset.add(&api, make_smsg(Address::default(), 6, 100), true, true);
        assert_eq!(
            res,
            Err(Error::NonceGap),
            "strict+trusted: gap of 5 (exceeds MAX_NONCE_GAP=4) should be rejected"
        );
    }

    #[test]
    fn test_strict_untrusted_rejects_any_gap() {
        let api = TestApi::default();
        let mut mset = MsgSet::new(0);

        // strict=true, trusted=false -> max_nonce_gap=0
        mset.add(&api, make_smsg(Address::default(), 0, 100), true, false)
            .unwrap();
        let res = mset.add(&api, make_smsg(Address::default(), 2, 100), true, false);
        assert_eq!(
            res,
            Err(Error::NonceGap),
            "strict+untrusted: any gap (maxNonceGap=0) is rejected"
        );
    }

    #[test]
    fn test_non_strict_untrusted_skips_gap_check() {
        let api = TestApi::default();
        let mut mset = MsgSet::new(0);

        // strict=false, trusted=false -> gap check skipped (PushUntrusted path)
        mset.add_untrusted(&api, make_smsg(Address::default(), 0, 100), false)
            .unwrap();
        let res = mset.add_untrusted(&api, make_smsg(Address::default(), 5, 100), false);
        assert!(
            res.is_ok(),
            "non-strict untrusted (PushUntrusted) skips gap enforcement"
        );
    }

    #[test]
    fn test_strict_rbf_during_gap_rejected() {
        let api = TestApi::default();
        let mut mset = MsgSet::new(0);

        // Set up a gap using non-strict trusted (local push path)
        mset.add_trusted(&api, make_smsg(Address::default(), 0, 100), false)
            .unwrap();
        mset.add_trusted(&api, make_smsg(Address::default(), 2, 100), false)
            .unwrap();

        // Strict RBF at nonce 2 should be rejected due to gap at nonce 1
        let res = mset.add(&api, make_smsg(Address::default(), 2, 200), true, true);
        assert_eq!(
            res,
            Err(Error::NonceGap),
            "strict RBF should be rejected when nonce gap exists"
        );
    }

    #[test]
    fn test_rbf_without_gap_still_works() {
        let api = TestApi::default();
        let mut mset = MsgSet::new(0);

        mset.add_trusted(&api, make_smsg(Address::default(), 0, 100), false)
            .unwrap();
        mset.add_trusted(&api, make_smsg(Address::default(), 1, 100), false)
            .unwrap();
        mset.add_trusted(&api, make_smsg(Address::default(), 2, 100), false)
            .unwrap();

        let res = mset.add_trusted(&api, make_smsg(Address::default(), 1, 200), false);
        assert!(res.is_ok(), "RBF without a nonce gap should succeed");
    }

    #[test]
    fn test_get_state_sequence_accounts_for_tipset_messages() {
        use crate::message_pool::test_provider::mock_block;

        let api = TestApi::default();
        let key_cache = SizeTrackingLruCache::new_mocked();
        let state_nonce_cache = SizeTrackingLruCache::new_mocked();

        let sender = Address::new_bls(&[3u8; 48]).unwrap();
        api.set_state_sequence(&sender, 5);

        let block = mock_block(1, 1);
        api.inner.lock().set_block_messages(
            &block,
            vec![make_smsg(sender, 5, 100), make_smsg(sender, 7, 100)],
        );
        let ts = Tipset::from(block);

        let nonce = get_state_sequence(&api, &key_cache, &state_nonce_cache, &sender, &ts).unwrap();
        assert_eq!(
            nonce, 8,
            "should account for non-consecutive tipset message at nonce 7"
        );
    }

    #[test]
    fn test_get_state_sequence_ignores_other_addresses() {
        use crate::message_pool::test_provider::mock_block;

        let api = TestApi::default();
        let key_cache = SizeTrackingLruCache::new_mocked();
        let state_nonce_cache = SizeTrackingLruCache::new_mocked();

        let addr_a = Address::new_bls(&[4u8; 48]).unwrap();
        let addr_b = Address::new_bls(&[5u8; 48]).unwrap();
        api.set_state_sequence(&addr_a, 0);
        api.set_state_sequence(&addr_b, 0);

        let block = mock_block(1, 1);
        api.inner.lock().set_block_messages(
            &block,
            vec![
                make_smsg(addr_b, 0, 100),
                make_smsg(addr_b, 1, 100),
                make_smsg(addr_b, 2, 100),
            ],
        );
        let ts = Tipset::from(block);

        let nonce_a =
            get_state_sequence(&api, &key_cache, &state_nonce_cache, &addr_a, &ts).unwrap();
        assert_eq!(
            nonce_a, 0,
            "addr_a nonce should be unaffected by addr_b's messages"
        );

        let nonce_b =
            get_state_sequence(&api, &key_cache, &state_nonce_cache, &addr_b, &ts).unwrap();
        assert_eq!(
            nonce_b, 3,
            "addr_b nonce should reflect its tipset messages"
        );
    }

    #[test]
    fn test_get_state_sequence_cache_hit() {
        use crate::message_pool::test_provider::mock_block;

        let api = TestApi::default();
        let key_cache = SizeTrackingLruCache::new_mocked();
        let state_nonce_cache: SizeTrackingLruCache<StateNonceCacheKey, u64> =
            SizeTrackingLruCache::new_mocked();

        let sender = Address::new_bls(&[6u8; 48]).unwrap();
        api.set_state_sequence(&sender, 5);

        let block = mock_block(1, 1);
        api.inner
            .lock()
            .set_block_messages(&block, vec![make_smsg(sender, 5, 100)]);
        let ts = Tipset::from(block);

        let nonce1 =
            get_state_sequence(&api, &key_cache, &state_nonce_cache, &sender, &ts).unwrap();
        assert_eq!(nonce1, 6);

        // Mutate the underlying state; the cache should still return the old value.
        api.set_state_sequence(&sender, 99);
        let nonce2 =
            get_state_sequence(&api, &key_cache, &state_nonce_cache, &sender, &ts).unwrap();
        assert_eq!(
            nonce2, 6,
            "second call should return the cached value, not re-read state"
        );
    }

    #[test]
    fn test_get_state_sequence_cache_miss_on_different_tipset() {
        use crate::message_pool::test_provider::mock_block;

        let api = TestApi::default();
        let key_cache = SizeTrackingLruCache::new_mocked();
        let state_nonce_cache: SizeTrackingLruCache<StateNonceCacheKey, u64> =
            SizeTrackingLruCache::new_mocked();

        let sender = Address::new_bls(&[7u8; 48]).unwrap();
        api.set_state_sequence(&sender, 10);

        let block_a = mock_block(1, 1);
        let ts_a = Tipset::from(&block_a);

        let nonce_a =
            get_state_sequence(&api, &key_cache, &state_nonce_cache, &sender, &ts_a).unwrap();
        assert_eq!(nonce_a, 10);

        // Different tipset should be a cache miss and re-read state.
        api.set_state_sequence(&sender, 20);
        let block_b = mock_block(2, 2);
        let ts_b = Tipset::from(&block_b);

        let nonce_b =
            get_state_sequence(&api, &key_cache, &state_nonce_cache, &sender, &ts_b).unwrap();
        assert_eq!(
            nonce_b, 20,
            "different tipset should miss the cache and read fresh state"
        );
    }
}
