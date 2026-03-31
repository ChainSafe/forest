// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub(in crate::message_pool) mod metrics;
pub(in crate::message_pool) mod msg_pool;
pub(in crate::message_pool) mod provider;
pub mod selection;
#[cfg(test)]
pub mod test_provider;
pub(in crate::message_pool) mod utils;

use std::{borrow::BorrowMut, cmp::Ordering};

use crate::blocks::Tipset;
use crate::libp2p::{NetworkMessage, PUBSUB_MSG_STR, Topic};
use crate::message::{MessageRead as _, SignedMessage};
use crate::networks::ChainConfig;
use crate::shim::{address::Address, crypto::Signature};
use crate::utils::cache::SizeTrackingLruCache;
use crate::utils::get_size::CidWrapper;
use ahash::{HashMap, HashMapExt, HashSet, HashSetExt};
use cid::Cid;
use fvm_ipld_encoding::to_vec;
use parking_lot::RwLock as SyncRwLock;
use tracing::error;
use utils::{get_base_fee_lower_bound, recover_sig};

use super::errors::Error;
use crate::message_pool::{
    msg_chain::{Chains, create_message_chains},
    msg_pool::{
        MsgSet, StateNonceCacheKey, TrustPolicy, add_helper, get_state_sequence, remove,
        resolve_to_key,
    },
    provider::Provider,
};

const REPLACE_BY_FEE_RATIO: f32 = 1.25;
const RBF_NUM: u64 = ((REPLACE_BY_FEE_RATIO - 1f32) * 256f32) as u64;
const RBF_DENOM: u64 = 256;
const BASE_FEE_LOWER_BOUND_FACTOR_CONSERVATIVE: i64 = 100;
const BASE_FEE_LOWER_BOUND_FACTOR: i64 = 10;
const REPUB_MSG_LIMIT: usize = 30;
const MIN_GAS: u64 = 1298450;

#[allow(clippy::too_many_arguments)]
async fn republish_pending_messages<T>(
    api: &T,
    network_sender: &flume::Sender<NetworkMessage>,
    pending: &SyncRwLock<HashMap<Address, MsgSet>>,
    cur_tipset: &SyncRwLock<Tipset>,
    republished: &SyncRwLock<HashSet<Cid>>,
    local_addrs: &SyncRwLock<Vec<Address>>,
    key_cache: &SizeTrackingLruCache<Address, Address>,
    chain_config: &ChainConfig,
) -> Result<(), Error>
where
    T: Provider,
{
    let ts = cur_tipset.read().clone();
    let mut pending_map: HashMap<Address, HashMap<u64, SignedMessage>> = HashMap::new();

    republished.write().clear();

    // Only republish messages from local addresses, ie. transactions which were
    // sent to this node directly.
    for actor in local_addrs.read().iter() {
        let resolved = resolve_to_key(api, key_cache, actor, &ts)?;
        if let Some(mset) = pending.read().get(&resolved) {
            if mset.msgs.is_empty() {
                continue;
            }
            let mut pend: HashMap<u64, SignedMessage> = HashMap::with_capacity(mset.msgs.len());
            for (nonce, m) in mset.msgs.clone().into_iter() {
                pend.insert(nonce, m);
            }
            pending_map.insert(resolved, pend);
        }
    }

    let msgs = select_messages_for_block(api, chain_config, &ts, pending_map)?;

    let network_name = chain_config.network.genesis_name();
    for m in msgs.iter() {
        let mb = to_vec(m)?;
        network_sender
            .send_async(NetworkMessage::PubsubMessage {
                topic: Topic::new(format!("{PUBSUB_MSG_STR}/{network_name}")),
                message: mb,
            })
            .await
            .map_err(|_| Error::Other("Network receiver dropped".to_string()))?;
    }

    let mut republished_t = HashSet::new();
    for m in msgs.iter() {
        republished_t.insert(m.cid());
    }
    *republished.write() = republished_t;

    Ok(())
}

/// Select messages from the mempool to be included in the next block that
/// builds on a given base tipset. The messages should be eligible for inclusion
/// based on their sequences and the overall number of them should observe block
/// gas limits.
fn select_messages_for_block<T>(
    api: &T,
    chain_config: &ChainConfig,
    base: &Tipset,
    pending: HashMap<Address, HashMap<u64, SignedMessage>>,
) -> Result<Vec<SignedMessage>, Error>
where
    T: Provider,
{
    let mut msgs: Vec<SignedMessage> = vec![];

    let base_fee = api.chain_compute_base_fee(base)?;
    let base_fee_lower_bound = get_base_fee_lower_bound(&base_fee, BASE_FEE_LOWER_BOUND_FACTOR);

    if pending.is_empty() {
        return Ok(msgs);
    }

    let mut chains = Chains::new();
    for (actor, mset) in pending.iter() {
        create_message_chains(
            api,
            actor,
            mset,
            &base_fee_lower_bound,
            base,
            &mut chains,
            chain_config,
        )?;
    }

    if chains.is_empty() {
        return Ok(msgs);
    }

    chains.sort(false);

    let mut gas_limit = crate::shim::econ::BLOCK_GAS_LIMIT;
    let mut i = 0;
    'l: while let Some(chain) = chains.get_mut_at(i) {
        // we can exceed this if we have picked (some) longer chain already
        if msgs.len() > REPUB_MSG_LIMIT {
            break;
        }

        if gas_limit <= MIN_GAS {
            break;
        }

        // check if chain has been invalidated
        if !chain.valid {
            i += 1;
            continue;
        }

        // check if fits in block
        if chain.gas_limit <= gas_limit {
            // check the baseFee lower bound -- only republish messages that can be included
            // in the chain within the next 20 blocks.
            for m in chain.msgs.iter() {
                if m.gas_fee_cap() < base_fee_lower_bound {
                    let key = chains.get_key_at(i);
                    chains.invalidate(key);
                    continue 'l;
                }
                gas_limit = gas_limit.saturating_sub(m.gas_limit());
                msgs.push(m.clone());
            }

            i += 1;
            continue;
        }

        // we can't fit the current chain but there is gas to spare
        // trim it and push it down
        chains.trim_msgs_at(i, gas_limit, REPUB_MSG_LIMIT, &base_fee);
        let mut j = i;
        while j < chains.len() - 1 {
            #[allow(clippy::indexing_slicing)]
            if chains[j].compare(&chains[j + 1]) == Ordering::Less {
                break;
            }
            chains.key_vec.swap(i, i + 1);
            j += 1;
        }
    }

    if msgs.len() > REPUB_MSG_LIMIT {
        msgs.truncate(REPUB_MSG_LIMIT);
    }

    Ok(msgs)
}

/// This function will revert and/or apply tipsets to the message pool. This
/// function should be called every time that there is a head change in the
/// message pool.
#[allow(clippy::too_many_arguments)]
pub async fn head_change<T>(
    api: &T,
    bls_sig_cache: &SizeTrackingLruCache<CidWrapper, Signature>,
    repub_trigger: flume::Sender<()>,
    republished: &SyncRwLock<HashSet<Cid>>,
    pending: &SyncRwLock<HashMap<Address, MsgSet>>,
    cur_tipset: &SyncRwLock<Tipset>,
    key_cache: &SizeTrackingLruCache<Address, Address>,
    state_nonce_cache: &SizeTrackingLruCache<StateNonceCacheKey, u64>,
    revert: Vec<Tipset>,
    apply: Vec<Tipset>,
) -> Result<(), Error>
where
    T: Provider + 'static,
{
    let mut repub = false;
    let mut rmsgs: HashMap<Address, HashMap<u64, SignedMessage>> = HashMap::new();
    for ts in revert {
        let Ok(pts) = api.load_tipset(ts.parents()) else {
            tracing::error!("error loading reverted tipset parent");
            continue;
        };
        *cur_tipset.write() = pts;

        let mut msgs: Vec<SignedMessage> = Vec::new();
        for block in ts.block_headers() {
            let Ok((umsg, smsgs)) = api.messages_for_block(block) else {
                tracing::error!("error retrieving messages for reverted block");
                continue;
            };
            msgs.extend(smsgs);
            for msg in umsg {
                let msg_cid = msg.cid();
                let Ok(smsg) = recover_sig(bls_sig_cache, msg) else {
                    tracing::debug!("could not recover signature for bls message {}", msg_cid);
                    continue;
                };
                msgs.push(smsg)
            }
        }

        for msg in msgs {
            add_to_selected_msgs(msg, rmsgs.borrow_mut());
        }
    }

    for ts in apply {
        for b in ts.block_headers() {
            let Ok((msgs, smsgs)) = api.messages_for_block(b) else {
                tracing::error!("error retrieving messages for block");
                continue;
            };

            for msg in smsgs {
                let cur_ts = cur_tipset.read().clone();
                remove_from_selected_msgs(
                    api,
                    key_cache,
                    &cur_ts,
                    &msg.from(),
                    pending,
                    msg.sequence(),
                    rmsgs.borrow_mut(),
                )?;
                if !repub && republished.write().insert(msg.cid()) {
                    repub = true;
                }
            }
            for msg in msgs {
                let cur_ts = cur_tipset.read().clone();
                remove_from_selected_msgs(
                    api,
                    key_cache,
                    &cur_ts,
                    &msg.from,
                    pending,
                    msg.sequence,
                    rmsgs.borrow_mut(),
                )?;
                if !repub && republished.write().insert(msg.cid()) {
                    repub = true;
                }
            }
        }
        *cur_tipset.write() = ts;
    }
    if repub {
        repub_trigger
            .send_async(())
            .await
            .map_err(|e| Error::Other(format!("Republish receiver dropped: {e}")))?;
    }
    for (_, hm) in rmsgs {
        for (_, msg) in hm {
            let cur_ts = cur_tipset.read().clone();
            let sequence =
                get_state_sequence(api, key_cache, state_nonce_cache, &msg.from(), &cur_ts)?;
            if let Err(e) = add_helper(
                api,
                bls_sig_cache,
                pending,
                key_cache,
                &cur_ts,
                msg,
                sequence,
                TrustPolicy::Trusted,
                false,
            ) {
                error!("Failed to read message from reorg to mpool: {}", e);
            }
        }
    }
    Ok(())
}

/// This is a helper function for `head_change`. This method will remove a
/// sequence for a from address from the messages selected by priority hash-map.
/// It also removes the 'from' address and sequence from the `MessagePool`.
pub(in crate::message_pool) fn remove_from_selected_msgs<T: Provider>(
    api: &T,
    key_cache: &SizeTrackingLruCache<Address, Address>,
    cur_ts: &Tipset,
    from: &Address,
    pending: &SyncRwLock<HashMap<Address, MsgSet>>,
    sequence: u64,
    rmsgs: &mut HashMap<Address, HashMap<u64, SignedMessage>>,
) -> Result<(), Error> {
    if let Some(temp) = rmsgs.get_mut(from) {
        if temp.get_mut(&sequence).is_some() {
            temp.remove(&sequence);
        } else {
            let resolved = resolve_to_key(api, key_cache, from, cur_ts)?;
            remove(&resolved, pending, sequence, true)?;
        }
    } else {
        let resolved = resolve_to_key(api, key_cache, from, cur_ts)?;
        remove(&resolved, pending, sequence, true)?;
    }
    Ok(())
}

/// This is a helper function for `head_change`. This method will add a signed
/// message to the given messages selected by priority `HashMap`.
pub(in crate::message_pool) fn add_to_selected_msgs(
    m: SignedMessage,
    rmsgs: &mut HashMap<Address, HashMap<u64, SignedMessage>>,
) {
    let s = rmsgs.get_mut(&m.from());
    if let Some(s) = s {
        s.insert(m.sequence(), m);
    } else {
        rmsgs.insert(m.from(), HashMap::new());
        rmsgs.get_mut(&m.from()).unwrap().insert(m.sequence(), m);
    }
}

#[cfg(test)]
pub mod tests {
    use std::{borrow::BorrowMut, time::Duration};

    use crate::blocks::Tipset;
    use crate::key_management::{KeyStore, KeyStoreConfig, Wallet};
    use crate::message::SignedMessage;
    use crate::networks::ChainConfig;
    use crate::shim::{
        address::Address,
        crypto::SignatureType,
        econ::TokenAmount,
        message::{Message, Message_v3},
    };
    use num_traits::Zero;
    use test_provider::*;
    use tokio::task::JoinSet;

    use super::*;
    use crate::message_pool::{
        msg_chain::{Chains, create_message_chains},
        msg_pool::MessagePool,
    };

    struct TestMpool {
        mpool: MessagePool<TestApi>,
        wallet: Wallet,
        sender: Address,
        target: Address,
        services: JoinSet<anyhow::Result<()>>,
        network_rx: flume::Receiver<NetworkMessage>,
    }

    fn make_test_mpool(
        tma: TestApi,
    ) -> (
        MessagePool<TestApi>,
        JoinSet<anyhow::Result<()>>,
        flume::Receiver<NetworkMessage>,
    ) {
        let (tx, rx) = flume::bounded(50);
        let mut services = JoinSet::new();
        let mpool = MessagePool::new(
            tma,
            tx,
            Default::default(),
            Default::default(),
            &mut services,
        )
        .unwrap();
        (mpool, services, rx)
    }

    fn make_test_setup() -> TestMpool {
        let keystore = KeyStore::new(KeyStoreConfig::Memory).unwrap();
        let mut wallet = Wallet::new(keystore);
        let sender = wallet.generate_addr(SignatureType::Secp256k1).unwrap();
        let target = wallet.generate_addr(SignatureType::Secp256k1).unwrap();
        let tma = TestApi::default();
        tma.set_state_sequence(&sender, 0);
        let (mpool, services, network_rx) = make_test_mpool(tma);
        TestMpool {
            mpool,
            wallet,
            sender,
            target,
            services,
            network_rx,
        }
    }

    #[tokio::test]
    async fn test_per_actor_limit() {
        let keystore = KeyStore::new(KeyStoreConfig::Memory).unwrap();
        let mut wallet = Wallet::new(keystore);
        let sender = wallet.generate_addr(SignatureType::Secp256k1).unwrap();
        let target = wallet.generate_addr(SignatureType::Secp256k1).unwrap();
        let tma = TestApi::with_max_actor_pending_messages(200);
        tma.set_state_sequence(&sender, 0);
        let (mpool, _services, _rx) = make_test_mpool(tma);

        let mut smsg_vec = Vec::new();
        for i in 0..(mpool.api.max_actor_pending_messages() + 1) {
            let msg = create_smsg(&target, &sender, wallet.borrow_mut(), i, 1000000, 1);
            smsg_vec.push(msg);
        }

        let (last, body) = smsg_vec.split_last().unwrap();
        for smsg in body {
            mpool.add(smsg.clone()).unwrap();
        }
        assert_eq!(
            mpool.add(last.clone()),
            Err(Error::TooManyPendingMessages(sender.to_string(), true))
        );
    }

    pub fn create_smsg(
        to: &Address,
        from: &Address,
        wallet: &mut Wallet,
        sequence: u64,
        gas_limit: i64,
        gas_price: u64,
    ) -> SignedMessage {
        let umsg: Message = Message_v3 {
            to: to.into(),
            from: from.into(),
            sequence,
            gas_limit: gas_limit as u64,
            gas_fee_cap: TokenAmount::from_atto(gas_price + 100).into(),
            gas_premium: TokenAmount::from_atto(gas_price).into(),
            ..Message_v3::default()
        }
        .into();
        let msg_signing_bytes = umsg.cid().to_bytes();
        let sig = wallet.sign(from, msg_signing_bytes.as_slice()).unwrap();
        SignedMessage::new_unchecked(umsg, sig)
    }

    // Create a fake signed message with a dummy signature. While the signature is
    // not valid, it has been added to the validation cache and the message will
    // appear authentic.
    pub fn create_fake_smsg(
        pool: &MessagePool<TestApi>,
        to: &Address,
        from: &Address,
        sequence: u64,
        gas_limit: i64,
        gas_price: u64,
    ) -> SignedMessage {
        let umsg: Message = Message_v3 {
            to: to.into(),
            from: from.into(),
            sequence,
            gas_limit: gas_limit as u64,
            gas_fee_cap: TokenAmount::from_atto(gas_price + 100).into(),
            gas_premium: TokenAmount::from_atto(gas_price).into(),
            ..Message_v3::default()
        }
        .into();
        let sig = Signature::new_secp256k1(vec![]);
        let signed = SignedMessage::new_unchecked(umsg, sig);
        let cid = signed.cid();
        pool.sig_val_cache.push(cid.into(), ());
        signed
    }

    #[tokio::test]
    async fn test_message_pool() {
        let TestMpool {
            mpool,
            mut wallet,
            sender,
            target,
            ..
        } = make_test_setup();

        let mut smsg_vec = Vec::new();
        for i in 0..2 {
            let msg = create_smsg(&target, &sender, wallet.borrow_mut(), i, 1000000, 1);
            smsg_vec.push(msg);
        }

        mpool.api.inner.lock().set_state_sequence(&sender, 0);
        assert_eq!(mpool.get_sequence(&sender).unwrap(), 0);
        mpool.add(smsg_vec[0].clone()).unwrap();
        assert_eq!(mpool.get_sequence(&sender).unwrap(), 1);
        mpool.add(smsg_vec[1].clone()).unwrap();
        assert_eq!(mpool.get_sequence(&sender).unwrap(), 2);

        let a = mock_block(1, 1);

        mpool.api.inner.lock().set_block_messages(&a, smsg_vec);
        mpool
            .apply_head_change(Vec::new(), vec![Tipset::from(a)])
            .await
            .unwrap();

        assert_eq!(mpool.get_sequence(&sender).unwrap(), 2);
    }

    #[tokio::test]
    async fn test_revert_messages() {
        let tma = TestApi::default();
        let keystore = KeyStore::new(KeyStoreConfig::Memory).unwrap();
        let mut wallet = Wallet::new(keystore);

        let a = mock_block(1, 1);
        let tipset = Tipset::from(&a);
        let b = mock_block_with_parents(&tipset, 1, 1);

        let sender = wallet.generate_addr(SignatureType::Bls).unwrap();
        let target = Address::new_id(1001);

        let mut smsg_vec = Vec::new();

        for i in 0..4 {
            let msg = create_smsg(&target, &sender, wallet.borrow_mut(), i, 1000000, 1);
            smsg_vec.push(msg);
        }
        let (mpool, _services, _rx) = make_test_mpool(tma);

        {
            let mut api_temp = mpool.api.inner.lock();
            api_temp.set_block_messages(&a, vec![smsg_vec[0].clone()]);
            api_temp.set_block_messages(&b.clone(), smsg_vec[1..4].to_vec());
            api_temp.set_state_sequence(&sender, 0);
            drop(api_temp);
        }

        mpool.add(smsg_vec[0].clone()).unwrap();
        mpool.add(smsg_vec[1].clone()).unwrap();
        mpool.add(smsg_vec[2].clone()).unwrap();
        mpool.add(smsg_vec[3].clone()).unwrap();

        mpool.api.set_state_sequence(&sender, 0);

        mpool
            .apply_head_change(Vec::new(), vec![Tipset::from(a)])
            .await
            .unwrap();

        assert_eq!(mpool.get_sequence(&sender).unwrap(), 4);

        mpool.api.set_state_sequence(&sender, 1);

        mpool
            .apply_head_change(Vec::new(), vec![Tipset::from(&b)])
            .await
            .unwrap();

        assert_eq!(mpool.get_sequence(&sender).unwrap(), 4);

        mpool.api.set_state_sequence(&sender, 0);

        mpool
            .apply_head_change(vec![Tipset::from(b)], Vec::new())
            .await
            .unwrap();

        assert_eq!(mpool.get_sequence(&sender).unwrap(), 4);

        let (p, _) = mpool.pending().unwrap();
        assert_eq!(p.len(), 3);
    }

    #[tokio::test]
    async fn test_get_sequence_resolves_id_address() {
        let tma = TestApi::default();
        let keystore = KeyStore::new(KeyStoreConfig::Memory).unwrap();
        let mut wallet = Wallet::new(keystore);
        let key_addr = wallet.generate_addr(SignatureType::Secp256k1).unwrap();
        let id_addr = Address::new_id(999);

        tma.set_key_address_mapping(&id_addr, &key_addr);
        tma.set_state_sequence(&key_addr, 0);
        let (mpool, _services, _rx) = make_test_mpool(tma);

        let target = Address::new_id(1001);
        for i in 0..3 {
            let msg = create_smsg(&target, &key_addr, wallet.borrow_mut(), i, 1000000, 1);
            mpool.add(msg).unwrap();
        }

        // get_sequence with ID address should see the same pending messages
        assert_eq!(mpool.get_sequence(&id_addr).unwrap(), 3);
        assert_eq!(mpool.get_sequence(&key_addr).unwrap(), 3);
    }

    #[tokio::test]
    async fn test_pending_for_resolves_id_address() {
        let tma = TestApi::default();
        let keystore = KeyStore::new(KeyStoreConfig::Memory).unwrap();
        let mut wallet = Wallet::new(keystore);
        let key_addr = wallet.generate_addr(SignatureType::Secp256k1).unwrap();
        let id_addr = Address::new_id(888);

        tma.set_key_address_mapping(&id_addr, &key_addr);
        tma.set_state_sequence(&key_addr, 0);
        let (mpool, _services, _rx) = make_test_mpool(tma);

        let target = Address::new_id(1001);
        for i in 0..2 {
            let msg = create_smsg(&target, &key_addr, wallet.borrow_mut(), i, 1000000, 1);
            mpool.add(msg).unwrap();
        }

        // pending_for with ID address should find messages added under key address
        let msgs = mpool
            .pending_for(&id_addr)
            .expect("should find pending messages");
        assert_eq!(msgs.len(), 2);

        // pending_for with key address should also work
        let msgs2 = mpool
            .pending_for(&key_addr)
            .expect("should find pending messages");
        assert_eq!(msgs2.len(), 2);
    }

    #[tokio::test]
    async fn test_add_with_id_from_resolves_to_key_in_pending() {
        let tma = TestApi::default();
        let key_addr = Address::new_bls(&[11u8; 48]).unwrap();
        let id_addr = Address::new_id(777);

        tma.set_key_address_mapping(&id_addr, &key_addr);
        tma.set_state_sequence(&key_addr, 0);
        let (mpool, _services, _rx) = make_test_mpool(tma);

        // Create a message with the ID address as sender and a fake signature
        let msg = create_fake_smsg(&mpool, &Address::new_id(1001), &id_addr, 0, 1000000, 1);
        mpool.add(msg).unwrap();

        // Pending map should be keyed by key_addr, not id_addr
        let pending = mpool.pending.read();
        assert!(
            pending.get(&key_addr).is_some(),
            "pending should be keyed by resolved key address"
        );
        assert!(
            pending.get(&id_addr).is_none(),
            "pending should NOT have entry under raw ID address"
        );
    }

    #[tokio::test]
    async fn test_head_change_removes_via_resolved_address() {
        let tma = TestApi::default();
        let keystore = KeyStore::new(KeyStoreConfig::Memory).unwrap();
        let mut wallet = Wallet::new(keystore);
        let key_addr = wallet.generate_addr(SignatureType::Bls).unwrap();
        let id_addr = Address::new_id(555);

        tma.set_key_address_mapping(&id_addr, &key_addr);
        tma.set_state_sequence(&key_addr, 0);

        let a = mock_block(1, 1);
        let (mpool, _services, _rx) = make_test_mpool(tma);

        let target = Address::new_id(1001);
        let msg0 = create_smsg(&target, &key_addr, wallet.borrow_mut(), 0, 1000000, 1);
        let msg1 = create_smsg(&target, &key_addr, wallet.borrow_mut(), 1, 1000000, 1);
        mpool.add(msg0.clone()).unwrap();
        mpool.add(msg1).unwrap();
        assert_eq!(mpool.get_sequence(&key_addr).unwrap(), 2);

        // Block messages are stored under the key_addr (as would appear on chain).
        // The head_change remove path resolves addresses before touching pending.
        mpool.api.inner.lock().set_block_messages(&a, vec![msg0]);

        mpool.api.set_state_sequence(&key_addr, 1);

        mpool
            .apply_head_change(Vec::new(), vec![Tipset::from(a)])
            .await
            .unwrap();

        // msg0 was applied on chain, msg1 remains pending
        assert_eq!(mpool.get_sequence(&id_addr).unwrap(), 2);
        let msgs = mpool
            .pending_for(&key_addr)
            .expect("should have remaining msg");
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].sequence(), 1);
    }

    #[tokio::test]
    async fn test_async_message_pool() {
        let TestMpool {
            mpool,
            mut wallet,
            sender,
            target,
            services: _services,
            network_rx: _network_rx,
        } = make_test_setup();

        let mut smsg_vec = Vec::new();
        for i in 0..3 {
            let msg = create_smsg(&target, &sender, wallet.borrow_mut(), i, 1000000, 1);
            smsg_vec.push(msg);
        }

        assert_eq!(mpool.get_sequence(&sender).unwrap(), 0);
        mpool.push(smsg_vec[0].clone()).await.unwrap();
        assert_eq!(mpool.get_sequence(&sender).unwrap(), 1);
        mpool.push(smsg_vec[1].clone()).await.unwrap();
        assert_eq!(mpool.get_sequence(&sender).unwrap(), 2);
        mpool.push(smsg_vec[2].clone()).await.unwrap();
        assert_eq!(mpool.get_sequence(&sender).unwrap(), 3);

        let header = mock_block(1, 1);
        let tipset = Tipset::from(&header.clone());

        mpool.api.set_heaviest_tipset(tipset.clone());

        // sleep allows for async block to update mpool's cur_tipset
        tokio::time::sleep(Duration::new(2, 0)).await;

        let cur_ts = mpool.current_tipset();
        assert_eq!(cur_ts, tipset);
    }

    #[tokio::test]
    async fn test_msg_chains() {
        let keystore = KeyStore::new(KeyStoreConfig::Memory).unwrap();
        let mut wallet = Wallet::new(keystore);
        let a1 = wallet.generate_addr(SignatureType::Secp256k1).unwrap();
        let a2 = wallet.generate_addr(SignatureType::Secp256k1).unwrap();
        let tma = TestApi::default();
        let gas_limit = 6955002;

        let a = mock_block(1, 1);
        let ts = Tipset::from(a);
        let chain_config = ChainConfig::default();

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

        let mut chains = Chains::new();
        create_message_chains(
            &tma,
            &a1,
            &mset,
            &TokenAmount::zero(),
            &ts,
            &mut chains,
            &chain_config,
        )
        .unwrap();
        assert_eq!(chains.len(), 1, "expected a single chain");
        assert_eq!(
            chains[0].msgs.len(),
            10,
            "expected 10 messages in single chain, got: {}",
            chains[0].msgs.len()
        );
        for (i, m) in chains[0].msgs.iter().enumerate() {
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
        let mut chains = Chains::new();
        create_message_chains(
            &tma,
            &a1,
            &mset,
            &TokenAmount::zero(),
            &ts,
            &mut chains,
            &chain_config,
        )
        .unwrap();
        assert_eq!(chains.len(), 10, "expected 10 chains");

        for i in 0..chains.len() {
            assert_eq!(
                chains[i].msgs.len(),
                1,
                "expected 1 message in chain {} but got {}",
                i,
                chains[i].msgs.len()
            );
        }

        for i in 0..chains.len() {
            let m = &chains[i].msgs[0];
            assert_eq!(
                m.sequence(),
                i as u64,
                "expected sequence {} but got {}",
                i,
                m.sequence()
            );
        }

        // Test 3a: 10 messages from a1 to a2, with gasPerf increasing in groups of 3;
        // it should          merge them in two chains, one with 9 messages and
        // one with the last message
        let mut mset = HashMap::new();
        let mut smsg_vec = Vec::new();
        for i in 0..10 {
            let msg = create_smsg(&a2, &a1, wallet.borrow_mut(), i, gas_limit, 1 + i % 3);
            smsg_vec.push(msg.clone());
            mset.insert(i, msg);
        }
        let mut chains = Chains::new();
        create_message_chains(
            &tma,
            &a1,
            &mset,
            &TokenAmount::zero(),
            &ts,
            &mut chains,
            &chain_config,
        )
        .unwrap();
        assert_eq!(chains.len(), 2, "expected 2 chains");
        assert_eq!(chains[0].msgs.len(), 9);
        assert_eq!(chains[1].msgs.len(), 1);
        let mut next_nonce = 0;
        for i in 0..chains.len() {
            for m in chains[i].msgs.iter() {
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

        // Test 3b: 10 messages from a1 to a2, with gasPerf decreasing in groups of 3
        // with a bias for the          earlier chains; it should make 4 chains,
        // the first 3 with 3 messages and the last with          a single
        // message
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

        let mut chains = Chains::new();
        create_message_chains(
            &tma,
            &a1,
            &mset,
            &TokenAmount::zero(),
            &ts,
            &mut chains,
            &chain_config,
        )
        .unwrap();

        for i in 0..chains.len() {
            let expected_len = if i > 2 { 1 } else { 3 };
            assert_eq!(
                chains[i].msgs.len(),
                expected_len,
                "expected {} message in chain {} but got {}",
                expected_len,
                i,
                chains[i].msgs.len()
            );
        }

        let mut next_nonce = 0;
        for i in 0..chains.len() {
            for m in chains[i].msgs.iter() {
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
        // Test 4: 10 messages with non-consecutive nonces; it should make a single
        // chain with just         the first message
        let mut mset = HashMap::new();
        let mut smsg_vec = Vec::new();
        for i in 0..10 {
            let msg = create_smsg(&a2, &a1, wallet.borrow_mut(), i * 2, gas_limit, 1 + i);
            smsg_vec.push(msg.clone());
            mset.insert(i, msg);
        }

        let mut chains = Chains::new();
        create_message_chains(
            &tma,
            &a1,
            &mset,
            &TokenAmount::zero(),
            &ts,
            &mut chains,
            &chain_config,
        )
        .unwrap();
        assert_eq!(chains.len(), 1, "expected a single chain");
        for (i, m) in chains[0].msgs.iter().enumerate() {
            assert_eq!(
                m.sequence(),
                i as u64,
                "expected nonce {} but got {}",
                i,
                m.sequence()
            );
        }

        // Test 5: 10 messages with increasing gasLimit, except for the 6th message
        // which has less than         the epoch gasLimit; it should create a
        // single chain with the first 5 messages
        let mut mset = HashMap::new();
        let mut smsg_vec = Vec::new();
        tma.set_state_balance_raw(&a1, TokenAmount::from_atto(1_000_000_000_000_000_000_u64));
        for i in 0..10 {
            let msg = if i != 5 {
                create_smsg(&a2, &a1, wallet.borrow_mut(), i, gas_limit, 1 + i)
            } else {
                create_smsg(&a2, &a1, wallet.borrow_mut(), i, 1, 1 + i)
            };
            smsg_vec.push(msg.clone());
            mset.insert(i, msg);
        }
        let mut chains = Chains::new();
        create_message_chains(
            &tma,
            &a1,
            &mset,
            &TokenAmount::zero(),
            &ts,
            &mut chains,
            &chain_config,
        )
        .unwrap();
        assert_eq!(chains.len(), 1, "expected a single chain");
        assert_eq!(chains[0].msgs.len(), 5);
        for (i, m) in chains[0].msgs.iter().enumerate() {
            assert_eq!(
                m.sequence(),
                i as u64,
                "expected nonce {} but got {}",
                i,
                m.sequence()
            );
        }

        // Test 6: one more message than what can fit in a block according to gas limit,
        // with increasing         gasPerf; it should create a single chain with
        // the max messages
        let mut mset = HashMap::new();
        let mut smsg_vec = Vec::new();
        let max_messages = crate::shim::econ::BLOCK_GAS_LIMIT as i64 / gas_limit;
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
        let mut chains = Chains::new();
        create_message_chains(
            &tma,
            &a1,
            &mset,
            &TokenAmount::zero(),
            &ts,
            &mut chains,
            &chain_config,
        )
        .unwrap();
        assert_eq!(chains.len(), 1, "expected a single chain");
        assert_eq!(chains[0].msgs.len(), max_messages as usize);
        for (i, m) in chains[0].msgs.iter().enumerate() {
            assert_eq!(
                m.sequence(),
                i as u64,
                "expected nonce {} but got {}",
                i,
                m.sequence()
            );
        }

        // Test 7: insufficient balance for all messages
        tma.set_state_balance_raw(&a1, TokenAmount::from_atto(300 * gas_limit + 1));
        let mut mset = HashMap::new();
        let mut smsg_vec = Vec::new();
        for i in 0..10 {
            let msg = create_smsg(&a2, &a1, wallet.borrow_mut(), i, gas_limit, 1 + i);
            smsg_vec.push(msg.clone());
            mset.insert(i, msg);
        }
        let mut chains = Chains::new();
        create_message_chains(
            &tma,
            &a1,
            &mset,
            &TokenAmount::zero(),
            &ts,
            &mut chains,
            &chain_config,
        )
        .unwrap();
        assert_eq!(chains.len(), 1, "expected a single chain");
        assert_eq!(chains[0].msgs.len(), 2);
        for (i, m) in chains[0].msgs.iter().enumerate() {
            assert_eq!(
                m.sequence(),
                i as u64,
                "expected nonce {} but got {}",
                i,
                m.sequence()
            );
        }
    }
}
