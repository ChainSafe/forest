// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

// Contains the implementation of Message Pool component.
// The Message Pool is the component of forest that handles pending messages for
// inclusion in the chain. Messages are added either directly for locally
// published messages or through pubsub propagation.

use std::{num::NonZeroUsize, sync::Arc, time::Duration};

use crate::blocks::{CachingBlockHeader, Tipset};
use crate::chain::{HeadChange, MINIMUM_BASE_FEE};
#[cfg(test)]
use crate::db::SettingsStore;
use crate::eth::is_valid_eth_tx_for_sending;
use crate::libp2p::{NetworkMessage, PUBSUB_MSG_STR, Topic};
use crate::message::{ChainMessage, Message, SignedMessage, valid_for_block_inclusion};
use crate::networks::{ChainConfig, NEWEST_NETWORK_VERSION};
use crate::shim::{
    address::Address,
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

pub const MAX_ACTOR_PENDING_MESSAGES: u64 = 1000;
pub const MAX_UNTRUSTED_ACTOR_PENDING_MESSAGES: u64 = 10;
/// Maximum size of a serialized message in bytes. This is an anti-DOS measure to prevent
/// large messages from being added to the message pool.
const MAX_MESSAGE_SIZE: usize = 64 << 10; // 64 KiB

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
    pub fn add_trusted<T>(&mut self, api: &T, m: SignedMessage) -> Result<(), Error>
    where
        T: Provider,
    {
        self.add(api, m, true)
    }

    /// Add a signed message to the `MsgSet`. Increase `next_sequence` if the
    /// message has a sequence greater than any existing message sequence.
    /// Use this method when pushing a message coming from untrusted sources.
    pub fn add_untrusted<T>(&mut self, api: &T, m: SignedMessage) -> Result<(), Error>
    where
        T: Provider,
    {
        self.add(api, m, false)
    }

    fn add<T>(&mut self, api: &T, m: SignedMessage, trusted: bool) -> Result<(), Error>
    where
        T: Provider,
    {
        let max_actor_pending_messages = if trusted {
            api.max_actor_pending_messages()
        } else {
            api.max_untrusted_actor_pending_messages()
        };

        if self.msgs.is_empty() || m.sequence() >= self.next_sequence {
            self.next_sequence = m.sequence() + 1;
        }

        let has_existing = if let Some(exms) = self.msgs.get(&m.sequence()) {
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
    /// A map of pending messages where the key is the address
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

impl<T> MessagePool<T>
where
    T: Provider,
{
    /// Gets the current tipset
    pub fn current_tipset(&self) -> Tipset {
        self.cur_tipset.read().clone()
    }

    /// Add a signed message to the pool and its address.
    fn add_local(&self, m: SignedMessage) -> Result<(), Error> {
        self.local_addrs.write().push(m.from());
        self.local_msgs.write().insert(m);
        Ok(())
    }

    /// Push a signed message to the `MessagePool`. Additionally performs basic
    /// checks on the validity of a message.
    pub async fn push_internal(&self, msg: SignedMessage, untrusted: bool) -> Result<Cid, Error> {
        self.check_message(&msg)?;
        let cid = msg.cid();
        let cur_ts = self.current_tipset();
        let publish = self.add_tipset(msg.clone(), &cur_ts, true, untrusted)?;
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
        self.push_internal(msg, false).await
    }

    /// Push a signed message to the `MessagePool` from an untrusted source.
    pub async fn push_untrusted(&self, msg: SignedMessage) -> Result<Cid, Error> {
        self.push_internal(msg, true).await
    }

    fn check_message(&self, msg: &SignedMessage) -> Result<(), Error> {
        if to_vec(msg)?.len() > MAX_MESSAGE_SIZE {
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
        let ts = self.current_tipset();
        self.add_tipset(msg, &ts, false, false)?;
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
        untrusted: bool,
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
        self.add_helper(msg, untrusted)?;
        Ok(publish)
    }

    /// Finish verifying signed message before adding it to the pending `mset`
    /// hash-map. If an entry in the hash-map does not yet exist, create a
    /// new `mset` that will correspond to the from message and push it to
    /// the pending hash-map.
    fn add_helper(&self, msg: SignedMessage, untrusted: bool) -> Result<(), Error> {
        let from = msg.from();
        let cur_ts = self.current_tipset();
        add_helper(
            self.api.as_ref(),
            self.bls_sig_cache.as_ref(),
            self.pending.as_ref(),
            msg,
            self.get_state_sequence(&from, &cur_ts)?,
            untrusted,
        )
    }

    /// Get the sequence for a given address, return Error if there is a failure
    /// to retrieve the respective sequence.
    pub fn get_sequence(&self, addr: &Address) -> Result<u64, Error> {
        let cur_ts = self.current_tipset();

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
        let pending = self.pending.read();
        let mset = pending.get(a)?;
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
        let repub_trigger = mp.repub_trigger.clone();

        // Reacts to new HeadChanges
        services.spawn(async move {
            loop {
                match subscriber.recv().await {
                    Ok(ts) => {
                        let (cur, rev, app) = match ts {
                            HeadChange::Apply(tipset) => {
                                (cur_tipset.clone(), Vec::new(), vec![tipset])
                            }
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
        let republish_interval = (10 * block_delay + chain_config.propagation_delay_secs) as u64;
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
pub(in crate::message_pool) fn add_helper<T>(
    api: &T,
    bls_sig_cache: &SizeTrackingLruCache<CidWrapper, Signature>,
    pending: &SyncRwLock<HashMap<Address, MsgSet>>,
    msg: SignedMessage,
    sequence: u64,
    untrusted: bool,
) -> Result<(), Error>
where
    T: Provider,
{
    if msg.signature().signature_type() == SignatureType::Bls {
        bls_sig_cache.push(msg.cid().into(), msg.signature().clone());
    }

    api.put_message(&ChainMessage::Signed(msg.clone()))?;
    api.put_message(&ChainMessage::Unsigned(msg.message().clone()))?;

    let mut pending = pending.write();
    let from = msg.from();
    let mset = pending.entry(from).or_insert_with(|| MsgSet::new(sequence));
    if untrusted {
        mset.add_untrusted(api, msg)?;
    } else {
        mset.add_trusted(api, msg)?;
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

    use super::*;
    use crate::shim::message::Message as ShimMessage;

    // Regression test for https://github.com/ChainSafe/forest/pull/6118 which fixed a bogus 100M
    // gas limit. There are no limits on a single message.
    #[test]
    fn add_helper_message_gas_limit_test() {
        let api = TestApi::default();
        let bls_sig_cache = SizeTrackingLruCache::new_mocked();
        let pending = SyncRwLock::new(HashMap::new());
        let message = ShimMessage {
            gas_limit: 666_666_666,
            ..ShimMessage::default()
        };
        let msg = SignedMessage::mock_bls_signed_message(message);
        let sequence = msg.message().sequence;
        let res = add_helper(&api, &bls_sig_cache, &pending, msg, sequence, false);
        assert!(res.is_ok());
    }

    // Test that RBF (Replace By Fee) is allowed even when at max_actor_pending_messages capacity
    // This matches Lotus behavior where the check is: https://github.com/filecoin-project/lotus/blob/5f32d00550ddd2f2d0f9abe97dbae07615f18547/chain/messagepool/messagepool.go#L296-L299
    #[test]
    fn test_rbf_at_capacity() {
        use crate::shim::econ::TokenAmount;

        let api = TestApi::with_max_actor_pending_messages(10);
        let mut mset = MsgSet::new(0);

        // Fill up to capacity (10 messages)
        for i in 0..10 {
            let message = ShimMessage {
                sequence: i,
                gas_premium: TokenAmount::from_atto(100u64),
                ..ShimMessage::default()
            };
            let msg = SignedMessage::mock_bls_signed_message(message);
            let res = mset.add_trusted(&api, msg);
            assert!(res.is_ok(), "Failed to add message {}: {:?}", i, res);
        }

        // Should reject adding a NEW message (sequence 10) when at capacity
        let message_new = ShimMessage {
            sequence: 10,
            gas_premium: TokenAmount::from_atto(100u64),
            ..ShimMessage::default()
        };
        let msg_new = SignedMessage::mock_bls_signed_message(message_new);
        let res_new = mset.add_trusted(&api, msg_new);
        assert!(matches!(res_new, Err(Error::TooManyPendingMessages(_, _))));

        // Should ALLOW replacing an existing message (RBF) even when at capacity
        // Replace message with sequence 5 with higher gas premium
        let message_rbf = ShimMessage {
            sequence: 5,
            gas_premium: TokenAmount::from_atto(200u64),
            ..ShimMessage::default()
        };
        let msg_rbf = SignedMessage::mock_bls_signed_message(message_rbf);
        let res_rbf = mset.add_trusted(&api, msg_rbf);
        assert!(
            res_rbf.is_ok(),
            "RBF should be allowed at capacity: {:?}",
            res_rbf
        );
    }
}
