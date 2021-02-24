// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod selection;
pub(crate) mod msg_pool;
pub(crate) mod provider;
pub(crate) mod utils;

use super::errors::Error;
use crate::msg_chain::{MsgChain, MsgChainNode};
use address::{Address, Protocol};
use async_std::channel::{bounded, Sender};
use async_std::stream::interval;
use async_std::sync::{Arc, RwLock};
use async_std::task;
use async_trait::async_trait;
use blocks::{BlockHeader, Tipset, TipsetKeys};
use blockstore::BlockStore;
use chain::{HeadChange, MINIMUM_BASE_FEE};
use cid::Cid;
use cid::Code::Blake2b256;
use crypto::{Signature, SignatureType};
use db::Store;
use encoding::Cbor;
use forest_libp2p::{NetworkMessage, Topic, PUBSUB_MSG_STR};
use futures::{future::select, StreamExt};
use log::{error, warn};
use lru::LruCache;
use message::{ChainMessage, Message, SignedMessage, UnsignedMessage};
use networks::{BLOCK_DELAY_SECS, NEWEST_NETWORK_VERSION};
use num_bigint::{BigInt, Integer};
use num_rational::BigRational;
use num_traits::cast::ToPrimitive;
use state_manager::StateManager;
use state_tree::StateTree;
use std::collections::{HashMap, HashSet};
use std::time::Duration;
use std::{borrow::BorrowMut, cmp::Ordering};
use tokio::sync::broadcast::{error::RecvError, Receiver as Subscriber, Sender as Publisher};
use types::verifier::ProofVerifier;
use utils::{get_base_fee_lower_bound, get_gas_reward, get_gas_perf};
use vm::ActorState;

const REPLACE_BY_FEE_RATIO: f32 = 1.25;
const RBF_NUM: u64 = ((REPLACE_BY_FEE_RATIO - 1f32) * 256f32) as u64;
const RBF_DENOM: u64 = 256;
const BASE_FEE_LOWER_BOUND_FACTOR_CONSERVATIVE: i64 = 100;
const BASE_FEE_LOWER_BOUND_FACTOR: i64 = 10;
const REPUB_MSG_LIMIT: usize = 30;
const PROPAGATION_DELAY_SECS: u64 = 6;
const REPUBLISH_INTERVAL: u64 = 10 * BLOCK_DELAY_SECS + PROPAGATION_DELAY_SECS;

/// Simple struct that contains a hashmap of messages where k: a message from address, v: a message
/// which corresponds to that address.
#[derive(Clone, Default, Debug)]
pub struct MsgSet {
    msgs: HashMap<u64, SignedMessage>,
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

/// Provider Trait. This trait will be used by the messagepool to interact with some medium in order to do
/// the operations that are listed below that are required for the messagepool.
#[async_trait]
pub trait Provider {
    /// Update Mpool's cur_tipset whenever there is a chnge to the provider
    async fn subscribe_head_changes(&mut self) -> Subscriber<HeadChange>;
    /// Get the heaviest Tipset in the provider
    async fn get_heaviest_tipset(&mut self) -> Option<Arc<Tipset>>;
    /// Add a message to the MpoolProvider, return either Cid or Error depending on successful put
    fn put_message(&self, msg: &ChainMessage) -> Result<Cid, Error>;
    /// Return state actor for given address given the tipset that the a temp StateTree will be rooted
    /// at. Return ActorState or Error depending on whether or not ActorState is found
    fn get_actor_after(&self, addr: &Address, ts: &Tipset) -> Result<ActorState, Error>;
    /// Return the signed messages for given blockheader
    fn messages_for_block(
        &self,
        h: &BlockHeader,
    ) -> Result<(Vec<UnsignedMessage>, Vec<SignedMessage>), Error>;
    /// Resolves to the key address
    async fn state_account_key<V>(
        &self,
        addr: &Address,
        ts: &Arc<Tipset>,
    ) -> Result<Address, Error>
    where
        V: ProofVerifier;
    /// Return all messages for a tipset
    fn messages_for_tipset(&self, h: &Tipset) -> Result<Vec<ChainMessage>, Error>;
    /// Return a tipset given the tipset keys from the ChainStore
    async fn load_tipset(&self, tsk: &TipsetKeys) -> Result<Arc<Tipset>, Error>;
    /// Computes the base fee
    fn chain_compute_base_fee(&self, ts: &Tipset) -> Result<BigInt, Error>;
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

/// Attempt to get a signed message that corresponds to an unsigned message in bls_sig_cache.
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
/// and push it to the pending hashmap.
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

/// Get the state of the base_sequence for a given address in cur_ts.
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

async fn republish_pending_messages<T>(
    api: &RwLock<T>,
    network_sender: &Sender<NetworkMessage>,
    network_name: &str,
    pending: &RwLock<HashMap<Address, MsgSet>>,
    cur_tipset: &RwLock<Arc<Tipset>>,
    republished: &RwLock<HashSet<Cid>>,
    local_addrs: &RwLock<Vec<Address>>,
) -> Result<(), Error>
where
    T: Provider,
{
    let ts = cur_tipset.read().await;
    let base_fee = api.read().await.chain_compute_base_fee(&ts)?;
    let base_fee_lower_bound = get_base_fee_lower_bound(&base_fee, BASE_FEE_LOWER_BOUND_FACTOR);
    let mut pending_map: HashMap<Address, HashMap<u64, SignedMessage>> = HashMap::new();

    republished.write().await.clear();
    let local_addrs = local_addrs.read().await;
    for actor in local_addrs.iter() {
        if let Some(mset) = pending.read().await.get(actor) {
            if mset.msgs.is_empty() {
                continue;
            }
            let mut pend: HashMap<u64, SignedMessage> = HashMap::with_capacity(mset.msgs.len());
            for (nonce, m) in mset.msgs.clone().into_iter() {
                pend.insert(nonce, m);
            }
            pending_map.insert(*actor, pend);
        }
    }
    drop(local_addrs);

    if pending_map.is_empty() {
        return Ok(());
    }

    let mut chains = Vec::new();
    for (actor, mset) in pending_map.iter() {
        let mut next = create_message_chains(&api, actor, mset, &base_fee_lower_bound, &ts).await?;
        chains.append(&mut next);
    }

    if chains.is_empty() {
        return Ok(());
    }

    chains.sort_by(|a, b| a.compare(b));

    let mut msgs: Vec<SignedMessage> = vec![];
    let mut gas_limit = types::BLOCK_GAS_LIMIT;
    let mut i = 0;
    'l: while i < chains.len() {
        let msg_chain = &mut chains[i];
        let chain = msg_chain.curr().clone();
        if msgs.len() > REPUB_MSG_LIMIT {
            break;
        }
        // TODO: Check min gas

        // check if chain has been invalidated
        if !chain.valid {
            i += 1;
            continue;
        }

        // check if fits in block
        if chain.gas_limit <= gas_limit {
            // check the baseFee lower bound -- only republish messages that can be included in the chain
            // within the next 20 blocks.
            for m in chain.msgs.iter() {
                if m.gas_fee_cap() < &base_fee_lower_bound {
                    msg_chain.invalidate();
                    continue 'l;
                }
                gas_limit -= m.gas_limit();
                msgs.push(m.clone());
            }
            i += 1;
            continue;
        }
        msg_chain.trim(gas_limit, &base_fee);
        let mut j = i;
        while j < chains.len() - 1 {
            if chains[j].compare(&chains[j + 1]) == Ordering::Less {
                break;
            }
            chains.swap(j, j + 1);
            j += 1;
        }
    }
    drop(ts);
    for m in msgs.iter() {
        let mb = m.marshal_cbor()?;
        network_sender
            .send(NetworkMessage::PubsubMessage {
                topic: Topic::new(format!("{}/{}", PUBSUB_MSG_STR, network_name)),
                message: mb,
            })
            .await
            .map_err(|_| Error::Other("Network receiver dropped".to_string()))?;
    }

    let mut republished_t = HashSet::new();
    for m in msgs.iter() {
        republished_t.insert(m.cid()?);
    }
    *republished.write().await = republished_t;

    Ok(())
}

async fn create_message_chains<T>(
    api: &RwLock<T>,
    actor: &Address,
    mset: &HashMap<u64, SignedMessage>,
    base_fee: &BigInt,
    ts: &Tipset,
) -> Result<Vec<MsgChain>, Error>
where
    T: Provider,
{
    // collect all messages and sort
    let mut msgs: Vec<SignedMessage> = mset.values().cloned().collect();
    msgs.sort_by_key(|v| v.sequence());

    // sanity checks:
    // - there can be no gaps in nonces, starting from the current actor nonce
    //   if there is a gap, drop messages after the gap, we can't include them
    // - all messages must have minimum gas and the total gas for the candidate messages
    //   cannot exceed the block limit; drop all messages that exceed the limit
    // - the total gasReward cannot exceed the actor's balance; drop all messages that exceed
    //   the balance
    let a = api.read().await.get_actor_after(&actor, &ts)?;

    let mut cur_seq = a.sequence;
    let mut balance = a.balance;
    let mut gas_limit = 0;

    let mut skip = 0;
    let mut i = 0;
    let mut rewards = Vec::with_capacity(msgs.len());
    while i < msgs.len() {
        let m = &msgs[i];
        if m.sequence() < cur_seq {
            warn!(
                "encountered message from actor {} with nonce {} less than the current nonce {}",
                actor,
                m.sequence(),
                cur_seq
            );
            skip += 1;
            i += 1;
            continue;
        }
        if m.sequence() != cur_seq {
            break;
        }
        cur_seq += 1;
        let min_gas = interpreter::price_list_by_epoch(ts.epoch())
            .on_chain_message(m.marshal_cbor()?.len())
            .total();
        if m.gas_limit() < min_gas {
            break;
        }
        gas_limit += m.gas_limit();

        if gas_limit > types::BLOCK_GAS_LIMIT {
            break;
        }
        let required = m.required_funds();
        if balance < required {
            break;
        }
        balance -= required;
        let value = m.value();
        if &balance >= value {
            balance -= value;
        }

        let gas_reward = get_gas_reward(&m, base_fee);
        rewards.push(gas_reward);
        i += 1;
    }
    // check we have a sane set of messages to construct the chains
    let msgs = if i > skip {
        msgs[skip..i].to_vec()
    } else {
        return Ok(vec![]);
    };

    let new_chain = |m: SignedMessage, i: usize| -> MsgChain {
        let gl = m.gas_limit();
        let node = MsgChainNode {
            msgs: vec![m],
            gas_reward: rewards[i].clone(),
            gas_limit: gl,
            gas_perf: get_gas_perf(&rewards[i], gl),
            eff_perf: 0.0,
            bp: 0.0,
            parent_offset: 0.0,
            valid: true,
            merged: false,
        };
        MsgChain::new(vec![node])
    };

    let mut chains = Vec::new();
    let mut cur_chain = MsgChain::default();

    for (i, m) in msgs.into_iter().enumerate() {
        if i == 0 {
            cur_chain = new_chain(m, i);
            continue;
        }
        let gas_reward = cur_chain.curr().gas_reward.clone() + &rewards[i];
        let gas_limit = cur_chain.curr().gas_limit + m.gas_limit();
        let gas_perf = get_gas_perf(&gas_reward, gas_limit);

        // try to add the message to the current chain -- if it decreases the gasPerf, then make a
        // new chain
        if gas_perf < cur_chain.curr().gas_perf {
            chains.push(cur_chain.clone());
            cur_chain = new_chain(m, i);
        } else {
            let cur = cur_chain.curr_mut();
            cur.msgs.push(m);
            cur.gas_reward = gas_reward;
            cur.gas_limit = gas_limit;
            cur.gas_perf = gas_perf;
        }
    }
    chains.push(cur_chain);

    // merge chains to maintain the invariant
    loop {
        let mut merged = 0;
        for i in (1..chains.len()).rev() {
            let (head, tail) = chains.split_at_mut(i);
            if tail[0].curr().gas_perf >= head.last().unwrap().curr().gas_perf {
                let mut chain_a_msgs = tail[0].curr().msgs.clone();
                head.last_mut()
                    .unwrap()
                    .curr_mut()
                    .msgs
                    .append(&mut chain_a_msgs);
                head.last_mut().unwrap().curr_mut().gas_reward += &tail[0].curr().gas_reward;
                head.last_mut().unwrap().curr_mut().gas_limit +=
                    head.last().unwrap().curr().gas_limit;
                head.last_mut().unwrap().curr_mut().gas_perf = get_gas_perf(
                    &head.last().unwrap().curr().gas_reward,
                    head.last().unwrap().curr().gas_limit,
                );
                tail[0].curr_mut().valid = false;
                merged += 1;
            }
        }
        if merged == 0 {
            break;
        }
        chains.retain(|c| c.curr().valid);
    }
    // No need to link the chains because its linked for free
    Ok(chains)
}

/// This function will revert and/or apply tipsets to the message pool. This function should be
/// called every time that there is a head change in the message pool.
#[allow(clippy::too_many_arguments)]
pub async fn head_change<T>(
    api: &RwLock<T>,
    bls_sig_cache: &RwLock<LruCache<Cid, Signature>>,
    repub_trigger: Arc<Sender<()>>,
    republished: &RwLock<HashSet<Cid>>,
    pending: &RwLock<HashMap<Address, MsgSet>>,
    cur_tipset: &RwLock<Arc<Tipset>>,
    revert: Vec<Tipset>,
    apply: Vec<Tipset>,
) -> Result<(), Error>
where
    T: Provider + 'static,
{
    let mut repub = false;
    let mut rmsgs: HashMap<Address, HashMap<u64, SignedMessage>> = HashMap::new();
    for ts in revert {
        let pts = api.write().await.load_tipset(ts.parents()).await?;
        *cur_tipset.write().await = pts;

        let mut msgs: Vec<SignedMessage> = Vec::new();
        for block in ts.blocks() {
            let (umsg, smsgs) = api.read().await.messages_for_block(&block)?;
            msgs.extend(smsgs);
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
                if !repub && republished.write().await.insert(msg.cid()?) {
                    repub = true;
                }
            }
            for msg in msgs {
                rm(msg.from(), pending, msg.sequence(), rmsgs.borrow_mut()).await?;
                if !repub && republished.write().await.insert(msg.cid()?) {
                    repub = true;
                }
            }
        }
        *cur_tipset.write().await = Arc::new(ts);
    }
    if repub {
        repub_trigger
            .send(())
            .await
            .map_err(|_| Error::Other("Republish receiver dropped".to_string()))?;
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

/// Like head_change, except it doesnt change the state of the MessagePool.
/// It simulates a head change call.
pub(crate) async fn run_head_change<T>(
    api: &RwLock<T>,
    pending: &RwLock<HashMap<Address, MsgSet>>,
    from: Tipset,
    to: Tipset,
    rmsgs: &mut HashMap<Address, HashMap<u64, SignedMessage>>,
) -> Result<(), Error>
where
    T: Provider + 'static,
{
    // TODO: This logic should probably be implemented in the ChainStore. It handles reorgs.
    let mut left = Arc::new(from);
    let mut right = Arc::new(to);
    let mut left_chain = Vec::new();
    let mut right_chain = Vec::new();
    while left != right {
        if left.epoch() > right.epoch() {
            left_chain.push(left.as_ref().clone());
            let par = api.read().await.load_tipset(left.parents()).await?;
            left = par;
        } else {
            right_chain.push(right.as_ref().clone());
            let par = api.read().await.load_tipset(right.parents()).await?;
            right = par;
        }
    }
    for ts in left_chain {
        let mut msgs: Vec<SignedMessage> = Vec::new();
        for block in ts.blocks() {
            let (_, smsgs) = api.read().await.messages_for_block(&block)?;
            msgs.extend(smsgs);
        }
        for msg in msgs {
            add(msg, rmsgs);
        }
    }

    for ts in right_chain {
        for b in ts.blocks() {
            let (msgs, smsgs) = api.read().await.messages_for_block(b)?;

            for msg in smsgs {
                rm(msg.from(), pending, msg.sequence(), rmsgs.borrow_mut()).await?;
            }
            for msg in msgs {
                rm(msg.from(), pending, msg.sequence(), rmsgs.borrow_mut()).await?;
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

/// This function is a helper method for head_change. This method will add a signed message to
/// the given rmsgs HashMap.
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
    use blocks::{BlockHeader, ElectionProof, Ticket, Tipset};
    use cid::Cid;
    use crypto::VRFProof;
    use message::{SignedMessage, UnsignedMessage};
    use std::convert::TryFrom;
    use tokio::sync::broadcast;

    /// Struct used for creating a provider when writing tests involving message pool
    pub struct TestApi {
        bmsgs: HashMap<Cid, Vec<SignedMessage>>,
        state_sequence: HashMap<Address, u64>,
        balances: HashMap<Address, BigInt>,
        tipsets: Vec<Tipset>,
        publisher: Publisher<HeadChange>,
    }

    impl Default for TestApi {
        /// Create a new TestApi
        fn default() -> Self {
            let (publisher, _) = broadcast::channel(1);
            TestApi {
                bmsgs: HashMap::new(),
                state_sequence: HashMap::new(),
                balances: HashMap::new(),
                tipsets: Vec::new(),
                publisher,
            }
        }
    }

    impl TestApi {
        /// Set the state sequence for an Address for TestApi
        pub fn set_state_sequence(&mut self, addr: &Address, sequence: u64) {
            self.state_sequence.insert(*addr, sequence);
        }

        /// Set the state balance for an Address for TestApi
        pub fn set_state_balance_raw(&mut self, addr: &Address, bal: BigInt) {
            self.balances.insert(*addr, bal);
        }

        /// Set the block messages for TestApi
        pub fn set_block_messages(&mut self, h: &BlockHeader, msgs: Vec<SignedMessage>) {
            self.bmsgs.insert(*h.cid(), msgs);
            self.tipsets.push(Tipset::new(vec![h.clone()]).unwrap())
        }

        /// Set the heaviest tipset for TestApi
        pub async fn set_heaviest_tipset(&mut self, ts: Arc<Tipset>) {
            self.publisher.send(HeadChange::Apply(ts)).unwrap();
        }

        pub fn next_block(&mut self) -> BlockHeader {
            let new_block = mock_block_with_parents(
                self.tipsets
                    .last()
                    .unwrap_or(&Tipset::new(vec![mock_block(1, 1)]).unwrap()),
                1,
                1,
            );
            new_block
        }
    }

    #[async_trait]
    impl Provider for TestApi {
        async fn subscribe_head_changes(&mut self) -> Subscriber<HeadChange> {
            self.publisher.subscribe()
        }

        async fn get_heaviest_tipset(&mut self) -> Option<Arc<Tipset>> {
            Tipset::new(vec![create_header(1)]).ok().map(Arc::new)
        }

        fn put_message(&self, _msg: &ChainMessage) -> Result<Cid, Errors> {
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
            let balance = match self.balances.get(addr) {
                Some(b) => b.clone(),
                None => (10_000_000_000_u64).into(),
            };

            msgs.sort_by_key(|m| m.sequence());
            let mut sequence: u64 = self.state_sequence.get(addr).copied().unwrap_or_default();
            for m in msgs {
                if m.sequence() != sequence {
                    break;
                }
                sequence += 1;
            }
            let actor = ActorState::new(Cid::default(), Cid::default(), balance, sequence);
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

        async fn state_account_key<V>(
            &self,
            addr: &Address,
            _ts: &Arc<Tipset>,
        ) -> Result<Address, Error> {
            match addr.protocol() {
                Protocol::BLS | Protocol::Secp256k1 => Ok(*addr),
                _ => Err(Error::Other("given address was not a key addr".to_string())),
            }
        }

        fn messages_for_tipset(&self, h: &Tipset) -> Result<Vec<ChainMessage>, Errors> {
            let (us, s) = self.messages_for_block(&h.blocks()[0]).unwrap();
            let mut msgs = Vec::new();

            for msg in us {
                msgs.push(ChainMessage::Unsigned(msg));
            }
            for smsg in s {
                msgs.push(ChainMessage::Signed(smsg));
            }
            Ok(msgs)
        }

        async fn load_tipset(&self, tsk: &TipsetKeys) -> Result<Arc<Tipset>, Errors> {
            for ts in &self.tipsets {
                if tsk.cids == ts.cids() {
                    return Ok(ts.clone().into());
                }
            }
            Err(Errors::InvalidToAddr)
        }

        fn chain_compute_base_fee(&self, _ts: &Tipset) -> Result<BigInt, Error> {
            Ok(100.into())
        }
    }

    pub fn create_header(weight: u64) -> BlockHeader {
        BlockHeader::builder()
            .weight(BigInt::from(weight))
            .miner_address(Address::new_id(0))
            .build()
            .unwrap()
    }

    pub fn mock_block(weight: u64, ticket_sequence: u64) -> BlockHeader {
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
            .message_receipts(c)
            .messages(c)
            .state_root(c)
            .weight(weight_inc)
            .build()
            .unwrap()
    }

    pub fn mock_block_with_epoch(epoch: i64, weight: u64, ticket_sequence: u64) -> BlockHeader {
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
            .message_receipts(c)
            .messages(c)
            .state_root(c)
            .weight(weight_inc)
            .epoch(epoch)
            .build()
            .unwrap()
    }
    pub fn mock_block_with_parents(
        parents: &Tipset,
        weight: u64,
        ticket_sequence: u64,
    ) -> BlockHeader {
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
            .message_receipts(c)
            .messages(c)
            .state_root(*parents.blocks()[0].state_root())
            .weight(weight_inc)
            .epoch(height)
            .build()
            .unwrap()
    }
}

#[cfg(test)]
pub mod tests {
    use super::test_provider::*;
    use super::*;
    use crate::msg_pool::MessagePool;
    use address::Address;
    use async_std::channel::bounded;
    use async_std::task;
    use blocks::Tipset;
    use crypto::SignatureType;
    use key_management::{MemKeyStore, Wallet};
    use message::{SignedMessage, UnsignedMessage};
    use num_bigint::BigInt;
    use std::borrow::BorrowMut;
    use std::thread::sleep;
    use std::time::Duration;

    pub fn create_smsg(
        to: &Address,
        from: &Address,
        wallet: &mut Wallet<MemKeyStore>,
        sequence: u64,
        gas_limit: i64,
        gas_price: u64,
    ) -> SignedMessage {
        let umsg: UnsignedMessage = UnsignedMessage::builder()
            .to(to.clone())
            .from(from.clone())
            .sequence(sequence)
            .gas_limit(gas_limit)
            .gas_fee_cap((gas_price + 100).into())
            .gas_premium(gas_price.into())
            .build()
            .unwrap();
        let msg_signing_bytes = umsg.to_signing_bytes();
        let sig = wallet.sign(&from, msg_signing_bytes.as_slice()).unwrap();
        let smsg = SignedMessage::new_from_parts(umsg, sig).unwrap();
        smsg.verify().unwrap();
        smsg
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
            let (tx, _rx) = bounded(50);
            let mpool = MessagePool::new(tma, "mptest".to_string(), tx, Default::default())
                .await
                .unwrap();
            let mut smsg_vec = Vec::new();
            for i in 0..2 {
                let msg = create_smsg(&target, &sender, wallet.borrow_mut(), i, 1000000, 1);
                smsg_vec.push(msg);
            }

            mpool.api.write().await.set_state_sequence(&sender, 0);
            assert_eq!(mpool.get_sequence(&sender).await.unwrap(), 0);
            mpool.add(smsg_vec[0].clone()).await.unwrap();
            assert_eq!(mpool.get_sequence(&sender).await.unwrap(), 1);
            mpool.add(smsg_vec[1].clone()).await.unwrap();
            assert_eq!(mpool.get_sequence(&sender).await.unwrap(), 2);

            let a = mock_block(1, 1);

            mpool.api.write().await.set_block_messages(&a, smsg_vec);
            let api = mpool.api.clone();
            let bls_sig_cache = mpool.bls_sig_cache.clone();
            let pending = mpool.pending.clone();
            let cur_tipset = mpool.cur_tipset.clone();
            let repub_trigger = Arc::new(mpool.repub_trigger.clone());
            let republished = mpool.republished.clone();
            head_change(
                api.as_ref(),
                bls_sig_cache.as_ref(),
                repub_trigger,
                republished.as_ref(),
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
        let b = mock_block_with_parents(&tipset, 1, 1);

        let sender = wallet.generate_addr(SignatureType::BLS).unwrap();
        let target = Address::new_id(1001);

        let mut smsg_vec = Vec::new();

        for i in 0..4 {
            let msg = create_smsg(&target, &sender, wallet.borrow_mut(), i, 1000000, 1);
            smsg_vec.push(msg);
        }
        let (tx, _rx) = bounded(50);

        task::block_on(async move {
            let mpool = MessagePool::new(tma, "mptest".to_string(), tx, Default::default())
                .await
                .unwrap();

            let mut api_temp = mpool.api.write().await;
            api_temp.set_block_messages(&a, vec![smsg_vec[0].clone()]);
            api_temp.set_block_messages(&b.clone(), smsg_vec[1..4].to_vec());
            api_temp.set_state_sequence(&sender, 0);
            drop(api_temp);

            mpool.add(smsg_vec[0].clone()).await.unwrap();
            mpool.add(smsg_vec[1].clone()).await.unwrap();
            mpool.add(smsg_vec[2].clone()).await.unwrap();
            mpool.add(smsg_vec[3].clone()).await.unwrap();

            mpool.api.write().await.set_state_sequence(&sender, 0);

            let api = mpool.api.clone();
            let bls_sig_cache = mpool.bls_sig_cache.clone();
            let pending = mpool.pending.clone();
            let cur_tipset = mpool.cur_tipset.clone();
            let repub_trigger = Arc::new(mpool.repub_trigger.clone());
            let republished = mpool.republished.clone();
            head_change(
                api.as_ref(),
                bls_sig_cache.as_ref(),
                repub_trigger.clone(),
                republished.as_ref(),
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
                repub_trigger.clone(),
                republished.as_ref(),
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
                repub_trigger.clone(),
                republished.as_ref(),
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
        let (tx, _rx) = bounded(50);

        task::block_on(async move {
            let mpool = MessagePool::new(tma, "mptest".to_string(), tx, Default::default())
                .await
                .unwrap();

            let mut smsg_vec = Vec::new();
            for i in 0..3 {
                let msg = create_smsg(&target, &sender, wallet.borrow_mut(), i, 1000000, 1);
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
            assert_eq!(cur_ts.as_ref(), &tipset);
        })
    }

    #[test]
    fn test_msg_chains() {
        let keystore = MemKeyStore::new();
        let mut wallet = Wallet::new(keystore);
        let a1 = wallet.generate_addr(SignatureType::Secp256k1).unwrap();
        let a2 = wallet.generate_addr(SignatureType::Secp256k1).unwrap();
        let tma = TestApi::default();
        let gas_limit = 6955002;
        task::block_on(async move {
            let tma = RwLock::new(tma);
            let a = mock_block(1, 1);
            let ts = Tipset::new(vec![a]).unwrap();

            // --- Test Chain Aggregations ---
            // Test 1: 10 messages from a1 to a2, with increasing gasPerf; it should
            // 	       make a single chain with 10 messages given enough balance
            let mut mset = HashMap::new();
            let mut smsg_vec = Vec::new();
            for i in 0..10 {
                let msg = create_smsg(&a2, &a1, wallet.borrow_mut(), i, gas_limit, 1 + i);
                smsg_vec.push(msg.clone());
                mset.insert(i, msg);
            }

            let chains = create_message_chains(&tma, &a1, &mset, &BigInt::from(0), &ts)
                .await
                .unwrap();
            assert_eq!(chains.len(), 1, "expected a single chain");
            assert_eq!(
                chains[0].curr().msgs.len(),
                10,
                "expected 10 messages in single chain, got: {}",
                chains[0].curr().msgs.len()
            );
            for (i, m) in chains[0].curr().msgs.iter().enumerate() {
                assert_eq!(
                    m.sequence(),
                    i as u64,
                    "expected sequence {} but got {}",
                    i,
                    m.sequence()
                );
            }

            // Test 2: 10 messages from a1 to a2, with decreasing gasPerf; it should
            // 	         make 10 chains with 1 message each
            let mut mset = HashMap::new();
            let mut smsg_vec = Vec::new();
            for i in 0..10 {
                let msg = create_smsg(&a2, &a1, wallet.borrow_mut(), i, gas_limit, 10 - i);
                smsg_vec.push(msg.clone());
                mset.insert(i, msg);
            }
            let chains = create_message_chains(&tma, &a1, &mset, &BigInt::from(0), &ts)
                .await
                .unwrap();
            assert_eq!(chains.len(), 10, "expected 10 chains");
            for (i, chain) in chains.iter().enumerate() {
                assert_eq!(
                    chain.curr().msgs.len(),
                    1,
                    "expected 1 message in chain {} but got {}",
                    i,
                    chain.curr().msgs.len()
                );
            }
            for (i, chain) in chains.iter().enumerate() {
                let m = &chain.curr().msgs[0];
                assert_eq!(
                    m.sequence(),
                    i as u64,
                    "expected sequence {} but got {}",
                    i,
                    m.sequence()
                );
            }

            // Test 3a: 10 messages from a1 to a2, with gasPerf increasing in groups of 3; it should
            //          merge them in two chains, one with 9 messages and one with the last message
            let mut mset = HashMap::new();
            let mut smsg_vec = Vec::new();
            for i in 0..10 {
                let msg = create_smsg(&a2, &a1, wallet.borrow_mut(), i, gas_limit, 1 + i % 3);
                smsg_vec.push(msg.clone());
                mset.insert(i, msg);
            }
            let chains = create_message_chains(&tma, &a1, &mset, &BigInt::from(0), &ts)
                .await
                .unwrap();
            assert_eq!(chains.len(), 2, "expected 2 chains");
            assert_eq!(chains[0].curr().msgs.len(), 9);
            assert_eq!(chains[1].curr().msgs.len(), 1);
            let mut next_nonce = 0;
            for chain in chains.iter() {
                for m in chain.curr().msgs.iter() {
                    assert_eq!(
                        next_nonce,
                        m.sequence(),
                        "expected nonce {} but got {}",
                        next_nonce,
                        m.sequence()
                    );
                    next_nonce += 1;
                }
            }

            // Test 3b: 10 messages from a1 to a2, with gasPerf decreasing in groups of 3 with a bias for the
            //          earlier chains; it should make 4 chains, the first 3 with 3 messages and the last with
            //          a single message
            let mut mset = HashMap::new();
            let mut smsg_vec = Vec::new();
            for i in 0..10 {
                let bias = (12 - i) / 3;
                let msg = create_smsg(
                    &a2,
                    &a1,
                    wallet.borrow_mut(),
                    i,
                    gas_limit,
                    1 + i % 3 + bias,
                );
                smsg_vec.push(msg.clone());
                mset.insert(i, msg);
            }
            let chains = create_message_chains(&tma, &a1, &mset, &BigInt::from(0), &ts)
                .await
                .unwrap();
            for (i, chain) in chains.iter().enumerate() {
                let expected_len = if i > 2 { 1 } else { 3 };
                assert_eq!(
                    chain.curr().msgs.len(),
                    expected_len,
                    "expected {} message in chain {} but got {}",
                    expected_len,
                    i,
                    chain.curr().msgs.len()
                );
            }
            let mut next_nonce = 0;
            for chain in chains.iter() {
                for m in chain.curr().msgs.iter() {
                    assert_eq!(
                        next_nonce,
                        m.sequence(),
                        "expected nonce {} but got {}",
                        next_nonce,
                        m.sequence()
                    );
                    next_nonce += 1;
                }
            }

            // --- Test Chain Breaks ---
            // Test 4: 10 messages with non-consecutive nonces; it should make a single chain with just
            //         the first message
            let mut mset = HashMap::new();
            let mut smsg_vec = Vec::new();
            for i in 0..10 {
                let msg = create_smsg(&a2, &a1, wallet.borrow_mut(), i * 2, gas_limit, 1 + i);
                smsg_vec.push(msg.clone());
                mset.insert(i, msg);
            }
            let chains = create_message_chains(&tma, &a1, &mset, &BigInt::from(0), &ts)
                .await
                .unwrap();
            assert_eq!(chains.len(), 1, "expected a single chain");
            for (i, m) in chains[0].curr().msgs.iter().enumerate() {
                assert_eq!(
                    m.sequence(),
                    i as u64,
                    "expected nonce {} but got {}",
                    i,
                    m.sequence()
                );
            }

            // Test 5: 10 messages with increasing gasLimit, except for the 6th message which has less than
            //         the epoch gasLimit; it should create a single chain with the first 5 messages
            let mut mset = HashMap::new();
            let mut smsg_vec = Vec::new();
            tma.write()
                .await
                .set_state_balance_raw(&a1, BigInt::from(1_000_000_000_000_000_000 as u64));
            for i in 0..10 {
                let msg = if i != 5 {
                    create_smsg(&a2, &a1, wallet.borrow_mut(), i, gas_limit, 1 + i)
                } else {
                    create_smsg(&a2, &a1, wallet.borrow_mut(), i, 1, 1 + i)
                };
                smsg_vec.push(msg.clone());
                mset.insert(i, msg);
            }
            let chains = create_message_chains(&tma, &a1, &mset, &BigInt::from(0), &ts)
                .await
                .unwrap();
            assert_eq!(chains.len(), 1, "expected a single chain");
            assert_eq!(chains[0].curr().msgs.len(), 5);
            for (i, m) in chains[0].curr().msgs.iter().enumerate() {
                assert_eq!(
                    m.sequence(),
                    i as u64,
                    "expected nonce {} but got {}",
                    i,
                    m.sequence()
                );
            }

            // Test 6: one more message than what can fit in a block according to gas limit, with increasing
            //         gasPerf; it should create a single chain with the max messages
            let mut mset = HashMap::new();
            let mut smsg_vec = Vec::new();
            let max_messages = types::BLOCK_GAS_LIMIT / gas_limit;
            let n_messages = max_messages + 1;
            for i in 0..n_messages {
                let msg = create_smsg(
                    &a2,
                    &a1,
                    wallet.borrow_mut(),
                    i as u64,
                    gas_limit,
                    (1 + i) as u64,
                );
                smsg_vec.push(msg.clone());
                mset.insert(i as u64, msg);
            }
            let chains = create_message_chains(&tma, &a1, &mset, &BigInt::from(0), &ts)
                .await
                .unwrap();
            assert_eq!(chains.len(), 1, "expected a single chain");
            assert_eq!(chains[0].curr().msgs.len(), max_messages as usize);
            for (i, m) in chains[0].curr().msgs.iter().enumerate() {
                assert_eq!(
                    m.sequence(),
                    i as u64,
                    "expected nonce {} but got {}",
                    i,
                    m.sequence()
                );
            }

            // Test 7: insufficient balance for all messages
            tma.write()
                .await
                .set_state_balance_raw(&a1, (300 * gas_limit + 1).into());
            let mut mset = HashMap::new();
            let mut smsg_vec = Vec::new();
            for i in 0..10 {
                let msg = create_smsg(&a2, &a1, wallet.borrow_mut(), i, gas_limit, 1 + i);
                smsg_vec.push(msg.clone());
                mset.insert(i as u64, msg);
            }
            let chains = create_message_chains(&tma, &a1, &mset, &BigInt::from(0), &ts)
                .await
                .unwrap();
            assert_eq!(chains.len(), 1, "expected a single chain");
            assert_eq!(chains[0].curr().msgs.len(), 2);
            for (i, m) in chains[0].curr().msgs.iter().enumerate() {
                assert_eq!(
                    m.sequence(),
                    i as u64,
                    "expected nonce {} but got {}",
                    i,
                    m.sequence()
                );
            }
        })
    }
}
