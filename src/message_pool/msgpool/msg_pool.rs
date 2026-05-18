// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

// Contains the implementation of Message Pool component.
// The Message Pool is the component of forest that handles pending messages for
// inclusion in the chain. Messages are added either directly for locally
// published messages or through pubsub propagation.

use crate::blocks::{CachingBlockHeader, Tipset, TipsetKey};
use crate::chain::{HeadChanges, MINIMUM_BASE_FEE};
use crate::eth::is_valid_eth_tx_for_sending;
use crate::libp2p::{NetworkMessage, PUBSUB_MSG_STR, Topic};
use crate::message::{ChainMessage, MessageRead as _, SignedMessage, valid_for_block_inclusion};
use crate::message_pool::{
    config::MpoolConfig,
    errors::Error,
    msgpool::{
        BASE_FEE_LOWER_BOUND_FACTOR_CONSERVATIVE, events::MpoolUpdate, pending_store::PendingStore,
        recover_sig, republish::RepublishState,
    },
    provider::Provider,
    utils::get_base_fee_lower_bound,
};
use crate::networks::{ChainConfig, NEWEST_NETWORK_VERSION};
use crate::prelude::*;
use crate::rpc::eth::types::EthAddress;
use crate::shim::{
    address::{Address, Protocol},
    crypto::{Signature, SignatureType},
    econ::TokenAmount,
    gas::{Gas, price_list_by_network_version},
    state_tree::ActorState,
};
use crate::state_manager::IdToAddressCache;
use crate::state_manager::utils::is_valid_for_sending;
use crate::utils::cache::SizeTrackingCache;
use ahash::HashSet;
use futures::StreamExt;
use fvm_ipld_encoding::to_vec;
use get_size2::GetSize;
use itertools::Itertools;
use nonzero_ext::nonzero;
use parking_lot::RwLock as SyncRwLock;
use std::num::NonZeroUsize;
use std::time::Duration;
use tokio::{
    sync::broadcast::{self, error::RecvError},
    task::JoinSet,
    time::interval,
};
use tracing::warn;

/// Maximum size of a serialized message in bytes. Anti-DoS measure to keep
/// the pool from ingesting pathologically large messages.
const MAX_MESSAGE_SIZE: usize = 64 << 10; // 64 KiB

pub(in crate::message_pool) const MAX_ACTOR_PENDING_MESSAGES: u64 = 1000;
pub(in crate::message_pool) const MAX_UNTRUSTED_ACTOR_PENDING_MESSAGES: u64 = 10;

// Cache sizes have been taken from the lotus implementation
const BLS_SIG_CACHE_SIZE: NonZeroUsize = nonzero!(40000usize);
const SIG_VAL_CACHE_SIZE: NonZeroUsize = nonzero!(32000usize);
const KEY_CACHE_SIZE: NonZeroUsize = nonzero!(1_048_576usize);
const STATE_NONCE_CACHE_SIZE: NonZeroUsize = nonzero!(32768usize);

#[derive(Clone, Debug, Hash, PartialEq, Eq, GetSize)]
pub(in crate::message_pool) struct StateNonceCacheKey {
    tipset_key: TipsetKey,
    addr: Address,
}

/// Trust policy for whether a message is from a trusted or untrusted source.
/// Untrusted sources are subject to stricter limits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::message_pool) enum TrustPolicy {
    Trusted,
    Untrusted,
}

pub use super::msg_set::{MsgSetLimits, StrictnessPolicy};

/// Caches owned by [`MessagePool`].
pub(in crate::message_pool) struct Caches {
    pub(in crate::message_pool) bls_sig: SizeTrackingCache<CidWrapper, Signature>,
    pub(in crate::message_pool) sig_val: SizeTrackingCache<CidWrapper, ()>,
    pub(in crate::message_pool) key: IdToAddressCache,
    pub(in crate::message_pool) state_nonce: SizeTrackingCache<StateNonceCacheKey, u64>,
}

impl Caches {
    pub(in crate::message_pool) fn new() -> Self {
        Self {
            bls_sig: SizeTrackingCache::new_with_metrics("bls_sig", BLS_SIG_CACHE_SIZE),
            sig_val: SizeTrackingCache::new_with_metrics("sig_val", SIG_VAL_CACHE_SIZE),
            key: SizeTrackingCache::new_with_metrics("mpool_key", KEY_CACHE_SIZE),
            state_nonce: SizeTrackingCache::new_with_metrics("state_nonce", STATE_NONCE_CACHE_SIZE),
        }
    }
}

impl ShallowClone for Caches {
    fn shallow_clone(&self) -> Self {
        Self {
            bls_sig: self.bls_sig.shallow_clone(),
            sig_val: self.sig_val.shallow_clone(),
            key: self.key.shallow_clone(),
            state_nonce: self.state_nonce.shallow_clone(),
        }
    }
}

/// This contains all necessary information needed for the message pool.
/// Keeps track of messages to apply, as well as context needed for verifying
/// transactions.
pub struct MessagePool<T> {
    /// Pending messages, keyed by resolved-key address, together with the
    /// broadcast channel for [`MpoolUpdate`] events. See [`PendingStore`].
    pub(in crate::message_pool) pending: PendingStore,
    pub(in crate::message_pool) caches: Caches,
    /// Resolved-key senders of locally submitted messages.
    pub(in crate::message_pool) local_addrs: Arc<SyncRwLock<HashSet<Address>>>,
    /// The current tipset (a set of blocks)
    pub(in crate::message_pool) cur_tipset: Arc<SyncRwLock<Tipset>>,
    /// The underlying provider
    pub(in crate::message_pool) api: Arc<T>,
    /// Sender half to send messages to other components
    pub(in crate::message_pool) network_sender: flume::Sender<NetworkMessage>,
    /// Republish coordination state
    pub(in crate::message_pool) republish: Arc<RepublishState>,
    /// Configurable parameters of the message pool.
    pub(in crate::message_pool) config: Arc<MpoolConfig>,
    /// Chain configuration
    pub(in crate::message_pool) chain_config: Arc<ChainConfig>,
}

impl<T> ShallowClone for MessagePool<T> {
    fn shallow_clone(&self) -> Self {
        Self {
            pending: self.pending.shallow_clone(),
            caches: self.caches.shallow_clone(),
            local_addrs: self.local_addrs.shallow_clone(),
            cur_tipset: self.cur_tipset.shallow_clone(),
            api: self.api.shallow_clone(),
            network_sender: self.network_sender.clone(),
            republish: self.republish.shallow_clone(),
            config: self.config.shallow_clone(),
            chain_config: self.chain_config.shallow_clone(),
        }
    }
}

/// Resolve an address to its key form, checking the cache first.
/// Non-ID addresses are returned unchanged.
pub(in crate::message_pool) fn resolve_to_key<T: Provider>(
    api: &T,
    key_cache: &IdToAddressCache,
    addr: &Address,
    cur_ts: &Tipset,
) -> Result<Address, Error> {
    let id = addr.id().ok();
    if let Some(id) = &id
        && let Some(resolved) = key_cache.get_cloned(id)
    {
        return Ok(resolved);
    }
    let resolved = api.resolve_to_deterministic_address_at_finality(addr, cur_ts)?;
    if let Some(id) = id {
        key_cache.push(id, resolved);
    }
    Ok(resolved)
}

impl<T> MessagePool<T>
where
    T: Provider,
{
    /// Gets the current tipset
    pub fn current_tipset(&self) -> Tipset {
        self.cur_tipset.read().clone()
    }

    pub(in crate::message_pool) fn resolve_to_key(
        &self,
        addr: &Address,
        cur_ts: &Tipset,
    ) -> Result<Address, Error> {
        resolve_to_key(self.api.as_ref(), &self.caches.key, addr, cur_ts)
    }

    /// Record the resolved-key sender of a locally-submitted message so the
    /// republish loop can find it on its next sweep.
    fn add_local(&self, m: &SignedMessage) -> Result<(), Error> {
        let cur_ts = self.current_tipset();
        let resolved = self.resolve_to_key(&m.from(), &cur_ts)?;
        self.local_addrs.write().insert(resolved);
        Ok(())
    }

    /// Push a signed message to the `MessagePool`. Records the sender as
    /// local and broadcasts on gossip if validation marks it publishable.
    async fn push_internal(
        &self,
        msg: SignedMessage,
        trust_policy: TrustPolicy,
    ) -> Result<Cid, Error> {
        let cid = msg.cid();
        let publish = self.add_to_pool(msg.clone(), true, trust_policy)?;
        self.add_local(&msg)?;
        if publish {
            self.publish_pubsub(&msg).await?;
        }
        Ok(cid)
    }

    /// Broadcast a signed message on the network's `gossipsub` topic.
    pub(in crate::message_pool) async fn publish_pubsub(
        &self,
        msg: &SignedMessage,
    ) -> Result<(), Error> {
        let message = to_vec(msg)?;
        let network_name = self.chain_config.network.genesis_name();
        self.network_sender
            .send_async(NetworkMessage::PubsubMessage {
                topic: Topic::new(format!("{PUBSUB_MSG_STR}/{network_name}")),
                message,
            })
            .await
            .map_err(|_| Error::Other("Network receiver dropped".to_string()))
    }

    /// Push a signed message to the `MessagePool` from an trusted source.
    pub async fn push(&self, msg: SignedMessage) -> Result<Cid, Error> {
        self.push_internal(msg, TrustPolicy::Trusted).await
    }

    /// Push a signed message to the `MessagePool` from an untrusted source.
    pub async fn push_untrusted(&self, msg: SignedMessage) -> Result<Cid, Error> {
        self.push_internal(msg, TrustPolicy::Untrusted).await
    }

    /// Insert a message received via gossip. Runs full validation. Does
    /// not publish back to the network.
    pub fn add(&self, msg: SignedMessage) -> Result<(), Error> {
        self.add_to_pool(msg, false, TrustPolicy::Trusted)?;
        Ok(())
    }

    /// Message validation.
    ///
    /// Returns `publish: bool` — `true` when the message should be gossiped
    /// after insertion; `false` when a local sender's message failed the
    /// soft base-fee floor (kept locally, not broadcast).
    pub(in crate::message_pool) fn validate_for_pool(
        &self,
        msg: &SignedMessage,
        cur_ts: &Tipset,
        local: bool,
    ) -> Result<bool, Error> {
        validate_static(msg)?;
        validate_signature(msg, &self.caches.sig_val, self.chain_config.eth_chain_id)?;

        let expected_sequence = self.get_state_sequence(&msg.from(), cur_ts)?;
        let sender_actor = self.api.get_actor_after(&msg.from(), cur_ts)?;

        validate_with_state(
            msg,
            &self.chain_config,
            cur_ts,
            &sender_actor,
            expected_sequence,
            local,
        )
    }

    /// Validate `msg` and insert it into the pending pool.
    ///
    /// Returns `publish: bool` (see [`Self::validate_for_pool`]).
    pub(in crate::message_pool) fn add_to_pool(
        &self,
        msg: SignedMessage,
        local: bool,
        trust_policy: TrustPolicy,
    ) -> Result<bool, Error> {
        let cur_ts = self.current_tipset();
        let publish = self.validate_for_pool(&msg, &cur_ts, local)?;
        let strictness = if local {
            StrictnessPolicy::Relaxed
        } else {
            StrictnessPolicy::Strict
        };
        self.add_to_pool_unchecked(&cur_ts, msg, trust_policy, strictness)?;
        Ok(publish)
    }

    /// Insert a message into the pending pool *without* running validation
    /// (size, sig, base-fee, sender-actor checks). The reorg replay path
    /// uses this directly to restore reverted messages even when they no
    /// longer pass the add-time filters.
    pub(in crate::message_pool) fn add_to_pool_unchecked(
        &self,
        cur_ts: &Tipset,
        msg: SignedMessage,
        trust_policy: TrustPolicy,
        strictness: StrictnessPolicy,
    ) -> Result<(), Error> {
        if msg.signature().signature_type() == SignatureType::Bls {
            self.caches
                .bls_sig
                .push(msg.cid().into(), msg.signature().clone());
        }

        self.api
            .put_message(&ChainMessage::Signed(msg.clone().into()))?;
        self.api
            .put_message(&ChainMessage::Unsigned(msg.message().clone().into()))?;

        let sequence = self.get_state_sequence(&msg.from(), cur_ts)?;
        let resolved_from = self.resolve_to_key(&msg.from(), cur_ts)?;
        self.pending
            .insert(resolved_from, msg, sequence, trust_policy, strictness)
    }

    /// Get the sequence for a given address, return Error if there is a failure
    /// to retrieve the respective sequence.
    pub fn get_sequence(&self, addr: &Address) -> Result<u64, Error> {
        let cur_ts = self.current_tipset();

        let sequence = self.get_state_sequence(addr, &cur_ts)?;

        let resolved = self.resolve_to_key(addr, &cur_ts).ok();
        let mset = resolved
            .and_then(|r| self.pending.snapshot_for(&r))
            .or_else(|| self.pending.snapshot_for(addr));
        match mset {
            Some(mset) => {
                if sequence > mset.next_sequence {
                    return Ok(sequence);
                }
                Ok(mset.next_sequence)
            }
            None => Ok(sequence),
        }
    }

    /// Get the state nonce for an address in `cur_ts`, accounting for
    /// messages already included in that tipset. Cached by `(TipsetKey,
    /// Address)`.
    pub(in crate::message_pool) fn get_state_sequence(
        &self,
        addr: &Address,
        cur_ts: &Tipset,
    ) -> Result<u64, Error> {
        let nk = StateNonceCacheKey {
            tipset_key: cur_ts.key().clone(),
            addr: *addr,
        };

        if let Some(cached) = self.caches.state_nonce.get_cloned(&nk) {
            return Ok(cached);
        }

        let actor = self.api.get_actor_after(addr, cur_ts)?;
        let mut next_nonce = actor.sequence;

        if let (Ok(resolved), Ok(messages)) = (
            self.resolve_to_key(addr, cur_ts)
                .inspect_err(|e| tracing::warn!(%addr, "failed to resolve address to key: {e:#}")),
            self.api
                .messages_for_tipset(cur_ts)
                .inspect_err(|e| tracing::warn!("failed to get messages for tipset: {e:#}")),
        ) {
            for msg in messages.iter() {
                if let Ok(from) = self.resolve_to_key(&msg.from(), cur_ts).inspect_err(
                    |e| tracing::warn!(from = %msg.from(), "failed to resolve message sender: {e:#}"),
                ) && from == resolved
                {
                    let n = msg.sequence() + 1;
                    if n > next_nonce {
                        next_nonce = n;
                    }
                }
            }
        }

        self.caches.state_nonce.push(nk, next_nonce);
        Ok(next_nonce)
    }

    /// Return a tuple that contains a vector of all signed messages and the
    /// current tipset for self.
    pub fn pending(&self) -> (Vec<SignedMessage>, Tipset) {
        let snapshot = self.pending.snapshot();
        let len = snapshot.values().map(|mset| mset.msgs.len()).sum();
        let mut out = Vec::with_capacity(len);

        for mset in snapshot.into_values() {
            out.extend(
                mset.msgs
                    .into_values()
                    .sorted_unstable_by_key(|m| m.message().sequence),
            );
        }

        let cur_ts = self.current_tipset();

        (out, cur_ts)
    }

    /// Return a Vector of signed messages for a given from address. This vector
    /// will be sorted by each `message`'s sequence. If no corresponding
    /// messages found, return None result type.
    pub fn pending_for(&self, a: &Address) -> Option<Vec<SignedMessage>> {
        let cur_ts = self.current_tipset();
        let resolved = self
            .resolve_to_key(a, &cur_ts)
            .inspect_err(|e| tracing::debug!(%a, "pending_for: failed to resolve address: {e:#}"))
            .ok()?;
        let mset = self.pending.snapshot_for(&resolved)?;
        if mset.msgs.is_empty() {
            return None;
        }

        Some(
            mset.msgs
                .into_values()
                .sorted_by_key(|v| v.message().sequence)
                .collect(),
        )
    }

    /// Subscribe to [`MpoolUpdate`] events for every insertion into and
    /// removal from the pending pool.
    #[allow(dead_code)] // surfaces the MpoolUpdate API for external subscribers.
    pub fn subscribe_to_updates(&self) -> broadcast::Receiver<MpoolUpdate> {
        self.pending.subscribe()
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
                let smsg = recover_sig(&self.caches.bls_sig, msg)?;
                msg_vec.push(smsg)
            }
        }
        Ok(msg_vec)
    }

    pub fn gas_limit_overestimation(&self) -> f64 {
        self.config.gas_limit_overestimation
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
    ) -> Result<Self, Error>
    where
        T: Provider,
    {
        // Per-actor limits are constant for the lifetime of this pool; capture
        // them once here rather than re-reading on every insert.
        let pending = PendingStore::new(MsgSetLimits::new(
            api.max_actor_pending_messages(),
            api.max_untrusted_actor_pending_messages(),
        ));
        let cur_tipset = Arc::new(SyncRwLock::new(api.get_heaviest_tipset()));
        let republish_interval =
            u64::from(10 * chain_config.block_delay_secs + chain_config.propagation_delay_secs);
        let (republish, repub_trigger_rx) = RepublishState::new();

        let mp = MessagePool {
            pending,
            caches: Caches::new(),
            local_addrs: Arc::new(SyncRwLock::new(HashSet::default())),
            republish: Arc::new(republish),
            cur_tipset,
            api: Arc::new(api),
            network_sender,
            config: Arc::new(config),
            chain_config,
        };

        // Reacts to new HeadChanges
        {
            let mp = mp.shallow_clone();
            let mut head_changes_rx = mp.api.subscribe_head_changes();
            services.spawn(async move {
                loop {
                    match head_changes_rx.recv().await {
                        Ok(HeadChanges { reverts, applies }) => {
                            if let Err(e) = mp.apply_head_change(reverts, applies).await {
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
        }

        // Reacts to republishing requests
        {
            let mp = mp.shallow_clone();
            services.spawn(async move {
                let mut repub_trigger_rx = repub_trigger_rx.stream();
                let mut interval = interval(Duration::from_secs(republish_interval));
                loop {
                    tokio::select! {
                        _ = interval.tick() => (),
                        _ = repub_trigger_rx.next() => (),
                    }
                    if let Err(e) = mp.run_republish_cycle().await {
                        warn!("Failed to republish pending messages: {}", e.to_string());
                    }
                }
            });
        }

        Ok(mp)
    }
}

fn validate_static(msg: &SignedMessage) -> Result<(), Error> {
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
    if msg.gas_fee_cap().atto() < &MINIMUM_BASE_FEE.into() {
        return Err(Error::GasFeeCapTooLow);
    }
    Ok(())
}

fn validate_signature(
    msg: &SignedMessage,
    sig_val_cache: &SizeTrackingCache<CidWrapper, ()>,
    eth_chain_id: u64,
) -> Result<(), Error> {
    let cid = msg.cid();
    if sig_val_cache.get_cloned(&CidWrapper::from(cid)).is_some() {
        return Ok(());
    }
    msg.verify(eth_chain_id)
        .map_err(|e| Error::Other(e.to_string()))?;
    sig_val_cache.push(CidWrapper::from(cid), ());
    Ok(())
}

/// Check the message against the pre-resolved chain state.
fn validate_with_state(
    msg: &SignedMessage,
    chain_config: &ChainConfig,
    cur_ts: &Tipset,
    sender_actor: &ActorState,
    expected_sequence: u64,
    local: bool,
) -> Result<bool, Error> {
    if expected_sequence > msg.message().sequence {
        return Err(Error::SequenceTooLow);
    }

    // The message can only be included in the next epoch and beyond, hence the +1.
    let nv_next = chain_config.network_version(cur_ts.epoch() + 1);
    if msg.is_delegated() && !is_valid_eth_tx_for_sending(chain_config.eth_chain_id, nv_next, msg) {
        return Err(Error::Other(
            "Invalid Ethereum message for the current network version".to_owned(),
        ));
    }
    if !is_valid_for_sending(nv_next, sender_actor) {
        return Err(Error::Other(
            "Sender actor is not a valid top-level sender".to_owned(),
        ));
    }

    let nv_cur = chain_config.network_version(cur_ts.epoch());
    let min_gas = price_list_by_network_version(nv_cur).on_chain_message(msg.chain_length()?);
    valid_for_block_inclusion(msg.message(), min_gas.total(), NEWEST_NETWORK_VERSION)?;

    let publish = check_base_fee_floor(msg, cur_ts, local)?;

    let balance = TokenAmount::from(&sender_actor.balance);
    if balance < msg.required_funds() {
        return Err(Error::NotEnoughFunds);
    }

    Ok(publish)
}

/// Base-Fee floor check.
pub(in crate::message_pool) fn check_base_fee_floor(
    msg: &SignedMessage,
    cur_ts: &Tipset,
    local: bool,
) -> Result<bool, Error> {
    let base_fee = &cur_ts.block_headers().first().parent_base_fee;
    let lb = get_base_fee_lower_bound(base_fee, BASE_FEE_LOWER_BOUND_FACTOR_CONSERVATIVE);
    if msg.gas_fee_cap() >= lb {
        return Ok(local);
    }
    if local {
        warn!(
            "local message will not be immediately published because GasFeeCap doesn't meet the lower bound for inclusion in the next 20 blocks (GasFeeCap: {}, baseFeeLowerBound: {})",
            msg.gas_fee_cap(),
            lb
        );
        return Ok(false);
    }
    Err(Error::SoftValidationFailure(format!(
        "GasFeeCap doesn't meet base fee lower bound for inclusion in the next 20 blocks (GasFeeCap: {}, baseFeeLowerBound:{})",
        msg.gas_fee_cap(),
        lb
    )))
}

#[cfg(test)]
mod tests {
    use crate::blocks::RawBlockHeader;
    use crate::chain::ChainStore;
    use crate::db::{DbImpl, MemoryDB};
    use crate::message_pool::provider::Provider;
    use crate::message_pool::test_provider::TestApi;
    use crate::networks::ChainConfig;
    use crate::shim::econ::TokenAmount;
    use crate::shim::state_tree::{ActorState, StateTree, StateTreeVersion};
    use crate::test_utils::dummy_ticket;
    use crate::utils::db::CborStoreExt as _;

    use super::*;
    use crate::shim::message::Message as ShimMessage;

    use tokio::task::JoinSet;

    fn make_smsg(from: Address, seq: u64, premium: u64) -> SignedMessage {
        SignedMessage::mock_bls_signed_message(ShimMessage {
            from,
            sequence: seq,
            gas_premium: TokenAmount::from_atto(premium),
            gas_limit: 1_000_000,
            ..ShimMessage::default()
        })
    }

    fn make_test_mpool(api: TestApi) -> (MessagePool<TestApi>, JoinSet<anyhow::Result<()>>) {
        let (tx, _rx) = flume::bounded(50);
        let mut services = JoinSet::new();
        let mpool = MessagePool::new(
            api,
            tx,
            Default::default(),
            Default::default(),
            &mut services,
        )
        .unwrap();
        (mpool, services)
    }

    // Regression test for https://github.com/ChainSafe/forest/pull/6118 which fixed a bogus 100M
    // gas limit. There are no limits on a single message.
    #[tokio::test]
    async fn add_to_pool_unchecked_accepts_high_gas_limit() {
        let api = TestApi::default();
        let (mpool, _services) = make_test_mpool(api);
        let cur_ts = mpool.current_tipset();
        let message = ShimMessage {
            gas_limit: 666_666_666,
            ..ShimMessage::default()
        };
        let msg = SignedMessage::mock_bls_signed_message(message);
        let res = mpool.add_to_pool_unchecked(
            &cur_ts,
            msg,
            TrustPolicy::Trusted,
            StrictnessPolicy::Relaxed,
        );
        assert!(res.is_ok());
    }

    #[tokio::test]
    async fn test_resolve_to_key_returns_non_id_unchanged() {
        let api = TestApi::default();
        let (mpool, _services) = make_test_mpool(api);
        let ts = mpool.current_tipset();

        let bls_addr = Address::new_bls(&[1u8; 48]).unwrap();
        let result = mpool.resolve_to_key(&bls_addr, &ts).unwrap();
        assert_eq!(result, bls_addr);
        assert_eq!(
            mpool.caches.key.len(),
            0,
            "cache should not be populated for non-ID addresses"
        );
    }

    #[tokio::test]
    async fn test_resolve_to_key_resolves_id_and_caches() {
        let api = TestApi::default();
        let id_addr = Address::new_id(100);
        let key_addr = Address::new_bls(&[5u8; 48]).unwrap();
        api.set_key_address_mapping(&id_addr, &key_addr);

        let (mpool, _services) = make_test_mpool(api);
        let ts = mpool.current_tipset();

        let result = mpool.resolve_to_key(&id_addr, &ts).unwrap();
        assert_eq!(result, key_addr);
        assert_eq!(
            mpool.caches.key.len(),
            1,
            "cache should have one entry after resolution"
        );

        // Second call should hit the cache (no API call needed)
        let result2 = mpool.resolve_to_key(&id_addr, &ts).unwrap();
        assert_eq!(result2, key_addr);
    }

    #[tokio::test]
    async fn test_add_to_pool_unchecked_keys_pending_by_resolved_address() {
        let api = TestApi::default();
        let id_addr = Address::new_id(200);
        let key_addr = Address::new_bls(&[7u8; 48]).unwrap();
        api.set_key_address_mapping(&id_addr, &key_addr);
        api.set_state_sequence(&key_addr, 0);

        let (mpool, _services) = make_test_mpool(api);
        let cur_ts = mpool.current_tipset();

        let message = ShimMessage {
            from: id_addr,
            gas_limit: 1_000_000,
            ..ShimMessage::default()
        };
        let msg = SignedMessage::mock_bls_signed_message(message);

        mpool
            .add_to_pool_unchecked(
                &cur_ts,
                msg,
                TrustPolicy::Trusted,
                StrictnessPolicy::Relaxed,
            )
            .unwrap();

        assert!(
            mpool.pending.snapshot_for(&key_addr).is_some(),
            "pending should be keyed by the resolved key address"
        );
        assert!(
            mpool.pending.snapshot_for(&id_addr).is_none(),
            "pending should NOT have an entry under the raw ID address"
        );
    }

    #[tokio::test]
    async fn test_get_sequence_works_with_both_address_forms() {
        let api = TestApi::default();
        let id_addr = Address::new_id(300);
        let key_addr = Address::new_bls(&[9u8; 48]).unwrap();
        api.set_key_address_mapping(&id_addr, &key_addr);
        api.set_state_sequence(&key_addr, 0);

        let (mpool, _services) = make_test_mpool(api);
        let cur_ts = mpool.current_tipset();

        // Add two messages from the ID address
        for seq in 0..2 {
            let message = ShimMessage {
                from: id_addr,
                sequence: seq,
                gas_limit: 1_000_000,
                ..ShimMessage::default()
            };
            let msg = SignedMessage::mock_bls_signed_message(message);
            mpool
                .add_to_pool_unchecked(
                    &cur_ts,
                    msg,
                    TrustPolicy::Trusted,
                    StrictnessPolicy::Relaxed,
                )
                .unwrap();
        }

        let state_seq = mpool
            .api
            .get_actor_after(&id_addr, &cur_ts)
            .unwrap()
            .sequence;
        let resolved_for_id = mpool.resolve_to_key(&id_addr, &cur_ts).unwrap();
        let resolved_for_key = mpool.resolve_to_key(&key_addr, &cur_ts).unwrap();
        assert_eq!(resolved_for_id, resolved_for_key);

        let next_seq = mpool
            .pending
            .snapshot_for(&resolved_for_id)
            .unwrap()
            .next_sequence;
        let expected = std::cmp::max(state_seq, next_seq);
        assert_eq!(expected, 2, "should reflect both pending messages");
    }

    #[tokio::test]
    async fn test_get_state_sequence_accounts_for_tipset_messages() {
        use crate::message_pool::test_provider::mock_block;

        let api = TestApi::default();
        let sender = Address::new_bls(&[3u8; 48]).unwrap();
        api.set_state_sequence(&sender, 5);

        let block = mock_block(1, 1);
        api.inner.lock().set_block_messages(
            &block,
            vec![make_smsg(sender, 5, 100), make_smsg(sender, 7, 100)],
        );
        let ts = Tipset::from(block);

        let (mpool, _services) = make_test_mpool(api);

        let nonce = mpool.get_state_sequence(&sender, &ts).unwrap();
        assert_eq!(
            nonce, 8,
            "should account for non-consecutive tipset message at nonce 7"
        );
    }

    #[tokio::test]
    async fn test_get_state_sequence_ignores_other_addresses() {
        use crate::message_pool::test_provider::mock_block;

        let api = TestApi::default();
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

        let (mpool, _services) = make_test_mpool(api);

        let nonce_a = mpool.get_state_sequence(&addr_a, &ts).unwrap();
        assert_eq!(
            nonce_a, 0,
            "addr_a nonce should be unaffected by addr_b's messages"
        );

        let nonce_b = mpool.get_state_sequence(&addr_b, &ts).unwrap();
        assert_eq!(
            nonce_b, 3,
            "addr_b nonce should reflect its tipset messages"
        );
    }

    #[tokio::test]
    async fn test_get_state_sequence_cache_hit() {
        use crate::message_pool::test_provider::mock_block;

        let api = TestApi::default();
        let sender = Address::new_bls(&[6u8; 48]).unwrap();
        api.set_state_sequence(&sender, 5);

        let block = mock_block(1, 1);
        api.inner
            .lock()
            .set_block_messages(&block, vec![make_smsg(sender, 5, 100)]);
        let ts = Tipset::from(block);

        let (mpool, _services) = make_test_mpool(api);

        let nonce1 = mpool.get_state_sequence(&sender, &ts).unwrap();
        assert_eq!(nonce1, 6);

        // Mutate the underlying state; the cache should still return the old value.
        mpool.api.set_state_sequence(&sender, 99);
        let nonce2 = mpool.get_state_sequence(&sender, &ts).unwrap();
        assert_eq!(
            nonce2, 6,
            "second call should return the cached value, not re-read state"
        );
    }

    #[tokio::test]
    async fn test_get_state_sequence_cache_miss_on_different_tipset() {
        use crate::message_pool::test_provider::mock_block;

        let api = TestApi::default();
        let sender = Address::new_bls(&[7u8; 48]).unwrap();
        api.set_state_sequence(&sender, 10);

        let (mpool, _services) = make_test_mpool(api);

        let block_a = mock_block(1, 1);
        let ts_a = Tipset::from(&block_a);

        let nonce_a = mpool.get_state_sequence(&sender, &ts_a).unwrap();
        assert_eq!(nonce_a, 10);

        // Different tipset should be a cache miss and re-read state.
        mpool.api.set_state_sequence(&sender, 20);
        let block_b = mock_block(2, 2);
        let ts_b = Tipset::from(&block_b);

        let nonce_b = mpool.get_state_sequence(&sender, &ts_b).unwrap();
        assert_eq!(
            nonce_b, 20,
            "different tipset should miss the cache and read fresh state"
        );
    }

    #[test]
    fn resolve_to_key_uses_finality_lookback() {
        let db: DbImpl = Arc::new(MemoryDB::default()).into();

        let mut cfg = ChainConfig::default();
        cfg.policy.chain_finality = 1;
        let cfg = Arc::new(cfg);

        let bls_a = Address::new_bls(&[8u8; 48]).unwrap();
        let bls_b = Address::new_bls(&[9u8; 48]).unwrap();

        // root_a: only contains f0300
        let mut st_a = StateTree::new(&db, StateTreeVersion::V5).unwrap();
        st_a.set_actor(
            &Address::new_id(300),
            ActorState::new_empty(Cid::default(), Some(bls_a)),
        )
        .unwrap();
        let root_a = st_a.flush().unwrap();

        // root_b: only contains f0400
        let mut st_b = StateTree::new(&db, StateTreeVersion::V5).unwrap();
        st_b.set_actor(
            &Address::new_id(400),
            ActorState::new_empty(Cid::default(), Some(bls_b)),
        )
        .unwrap();
        let root_b = st_b.flush().unwrap();

        let genesis = Tipset::from(CachingBlockHeader::new(RawBlockHeader {
            ticket: dummy_ticket(0),
            state_root: root_a,
            ..Default::default()
        }));
        db.put_cbor_default(genesis.block_headers().first())
            .unwrap();

        let ts1 = Tipset::from(CachingBlockHeader::new(RawBlockHeader {
            parents: genesis.key().clone(),
            ticket: dummy_ticket(1),
            epoch: 1,
            state_root: root_a,
            timestamp: 1,
            ..Default::default()
        }));
        db.put_cbor_default(ts1.block_headers().first()).unwrap();

        let head = Tipset::from(CachingBlockHeader::new(RawBlockHeader {
            parents: ts1.key().clone(),
            ticket: dummy_ticket(2),
            epoch: 2,
            state_root: root_b,
            timestamp: 2,
            ..Default::default()
        }));
        db.put_cbor_default(head.block_headers().first()).unwrap();

        let cs = ChainStore::new(db, cfg, genesis.block_headers().first().clone()).unwrap();

        // f0300 exists in lookback state (root_a) → resolves successfully.
        let result = Provider::resolve_to_deterministic_address_at_finality(
            &cs,
            &Address::new_id(300),
            &head,
        )
        .unwrap();
        assert_eq!(result, bls_a);

        // f0400 exists only in head state (root_b), not in lookback → fails.
        Provider::resolve_to_deterministic_address_at_finality(&cs, &Address::new_id(400), &head)
            .expect_err("actor only in head state must not resolve via finality lookback");
    }
}
