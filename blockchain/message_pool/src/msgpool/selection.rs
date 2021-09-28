// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Contains routines for message selection APIs.
//! Whenever a miner is ready to create a block for a tipset, it invokes the select_messages API
//! which selects an appropriate set of messages such that it optimizes miner reward and chain capacity.
//! See https://docs.filecoin.io/mine/lotus/message-pool/#message-selection for more details

use super::msg_pool::MessagePool;
use super::provider::Provider;
use crate::msg_chain::{create_message_chains, Chains, NodeKey};
use crate::msg_pool::MsgSet;
use crate::msgpool::MIN_GAS;
use crate::Error;
use crate::{add_to_selected_msgs, remove_from_selected_msgs};
use address::Address;
use async_std::sync::{Arc, RwLock};
use blocks::Tipset;
use message::Message;
use message::SignedMessage;
use num_bigint::BigInt;
use rand::prelude::SliceRandom;
use rand::thread_rng;
use std::borrow::BorrowMut;
use std::cmp::Ordering;
use std::collections::HashMap;

type Pending = HashMap<Address, HashMap<u64, SignedMessage>>;

// A cap on maximum number of message to include in a block
const MAX_BLOCK_MSGS: usize = 16000;
const MAX_BLOCKS: usize = 15;

impl<T> MessagePool<T>
where
    T: Provider + Send + Sync + 'static,
{
    /// Forest employs a sophisticated algorithm for selecting messages
    /// for inclusion from the pool, given the ticket quality of a miner.
    /// This method selects messages for including in a block.
    pub async fn select_messages(&self, ts: &Tipset, tq: f64) -> Result<Vec<SignedMessage>, Error> {
        let cur_ts = self.cur_tipset.read().await.clone();
        // if the ticket quality is high enough that the first block has higher probability
        // than any other block, then we don't bother with optimal selection because the
        // first block will always have higher effective performance. Otherwise we select
        // message optimally based on effective performance of chains.
        let mut msgs = if tq > 0.84 {
            self.select_messages_greedy(&cur_ts, ts).await
        } else {
            self.select_messages_optimal(&cur_ts, ts, tq).await
        }?;

        if msgs.len() > MAX_BLOCK_MSGS {
            msgs.truncate(MAX_BLOCK_MSGS)
        }

        Ok(msgs)
    }

    async fn select_messages_greedy(
        &self,
        cur_ts: &Tipset,
        ts: &Tipset,
    ) -> Result<Vec<SignedMessage>, Error> {
        let base_fee = self.api.read().await.chain_compute_base_fee(&ts)?;

        // 0. Load messages from the target tipset; if it is the same as the current tipset in
        //    the mpool, then this is just the pending messages
        let mut pending = self.get_pending_messages(&cur_ts, &ts).await?;

        if pending.is_empty() {
            return Ok(Vec::new());
        }
        // 0b. Select all priority messages that fit in the block
        let (result, gas_limit) = self
            .select_priority_messages(&mut pending, &base_fee, &ts)
            .await?;

        // check if block has been filled
        if gas_limit < MIN_GAS {
            return Ok(result);
        }

        // 1. Create a list of dependent message chains with maximal gas reward per limit consumed
        let mut chains = Chains::new();
        for (actor, mset) in pending.into_iter() {
            create_message_chains(&self.api, &actor, &mset, &base_fee, &ts, &mut chains).await?;
        }

        let (msgs, _) = merge_and_trim(&mut chains, result, &base_fee, gas_limit, MIN_GAS);
        Ok(msgs)
    }

    async fn select_messages_optimal(
        &self,
        cur_ts: &Tipset,
        target_tipset: &Tipset,
        ticket_quality: f64,
    ) -> Result<Vec<SignedMessage>, Error> {
        let base_fee = self
            .api
            .read()
            .await
            .chain_compute_base_fee(&target_tipset)?;

        // 0. Load messages from the target tipset; if it is the same as the current tipset in
        //    the mpool, then this is just the pending messages
        let mut pending = self.get_pending_messages(&cur_ts, &target_tipset).await?;

        if pending.is_empty() {
            return Ok(Vec::new());
        }

        // 0b. Select all priority messages that fit in the block
        let (mut result, mut gas_limit) = self
            .select_priority_messages(&mut pending, &base_fee, &target_tipset)
            .await?;

        // check if block has been filled
        if gas_limit < MIN_GAS {
            return Ok(result);
        }

        // 1. Create a list of dependent message chains with maximal gas reward per limit consumed
        let mut chains = Chains::new();
        for (actor, mset) in pending.into_iter() {
            create_message_chains(
                &self.api,
                &actor,
                &mset,
                &base_fee,
                &target_tipset,
                &mut chains,
            )
            .await?;
        }

        // 2. Sort the chains
        chains.sort(false);

        if !chains.is_empty() && chains[0].gas_perf < 0.0 {
            log::warn!(
                "all messages in mpool have non-positive gas performance {}",
                chains[0].gas_perf
            );
            return Ok(result);
        }

        // 3. Parition chains into blocks (without trimming)
        //    we use the full block_gas_limit (as opposed to the residual `gas_limit` from the
        //    priority message selection) as we have to account for what other miners are doing
        let mut next_chain = 0;
        let mut partitions: Vec<Vec<NodeKey>> = vec![vec![]; MAX_BLOCKS];
        let mut i = 0;
        while i < MAX_BLOCKS && next_chain < chains.len() {
            let mut gas_limit = types::BLOCK_GAS_LIMIT;
            while next_chain < chains.len() {
                let chain_key = chains.key_vec[next_chain];
                next_chain += 1;
                partitions[i].push(chain_key);
                gas_limit -= chains.get(chain_key).unwrap().gas_limit;
                if gas_limit < MIN_GAS {
                    break;
                }
            }
            i += 1;
        }

        // 4. Compute effective performance for each chain, based on the partition they fall into
        //    The effective performance is the gas_perf of the chain * block probability
        let block_prob = crate::block_probabilities(ticket_quality);
        let mut eff_chains = 0;
        for i in 0..MAX_BLOCKS {
            for k in &partitions[i] {
                if let Some(node) = chains.get_mut(*k) {
                    node.eff_perf = node.gas_perf * block_prob[i];
                }
            }
            eff_chains += partitions[i].len();
        }

        // nullify the effective performance of chains that don't fit in any partition
        for i in eff_chains..chains.len() {
            if let Some(node) = chains.get_mut_at(i) {
                node.set_null_effective_perf();
            }
        }

        // 5. Re-sort the chains based on effective performance
        chains.sort_effective();

        // 6. Merge the head chains to produce the list of messages selected for inclusion
        //    subject to the residual gas limit
        //    When a chain is merged in, all its previous dependent chains *must* also be
        //    merged in or we'll have a broken block
        let mut last = chains.len();
        for i in 0..chains.len() {
            // did we run out of performing chains?
            if chains[i].gas_perf < 0.0 {
                break;
            }

            // has it already been merged?
            if chains[i].merged {
                continue;
            }

            // compute the dependencies that must be merged and the gas limit including dependencies
            let mut chain_gas_limit = chains[i].gas_limit;
            let mut chain_deps = vec![];
            let mut cur_chain = chains[i].prev;
            while let Some(cur_chn) = cur_chain {
                let node = chains.get(cur_chn).unwrap();
                if !node.merged {
                    chain_deps.push(cur_chn);
                    chain_gas_limit += node.gas_limit;
                    cur_chain = node.prev;
                } else {
                    break;
                }
            }

            // does it all fit in the block?
            if chain_gas_limit <= gas_limit {
                // include it together with all dependencies
                chain_deps.iter().rev().for_each(|dep| {
                    if let Some(node) = chains.get_mut(*dep) {
                        node.merged = true;
                        result.extend(node.msgs.clone());
                    }
                });

                chains[i].merged = true;

                // adjust the effective peformance for all subsequent chains
                if let Some(next_key) = chains[i].next {
                    let mut next_node = chains.get_mut(next_key).unwrap();
                    if next_node.eff_perf > 0.0 {
                        next_node.eff_perf += next_node.parent_offset;
                        let mut next_next_key = next_node.next;
                        while let Some(nnk) = next_next_key {
                            let (nn_node, prev_perfs) = chains.get_mut_with_prev_eff(nnk);
                            if let Some(nn_node) = nn_node {
                                if nn_node.eff_perf > 0.0 {
                                    nn_node.set_eff_perf(prev_perfs);
                                    next_next_key = nn_node.next;
                                } else {
                                    break;
                                }
                            } else {
                                break;
                            }
                        }
                    }
                }

                result.extend(chains[i].msgs.clone());
                gas_limit -= chain_gas_limit;

                // re-sort to account for already merged chains and effective performance adjustments
                // the sort *must* be stable or we end up getting negative gasPerfs pushed up.
                chains.sort_range_effective(i + 1..);

                continue;
            }

            // we can't fit this chain and its dependencies because of block gasLimit -- we are
            // at the edge
            last = i;
            break;
        }

        // 7. We have reached the edge of what can fit wholesale; if we still hae available
        //    gasLimit to pack some more chains, then trim the last chain and push it down.
        //    Trimming invalidaates subsequent dependent chains so that they can't be selected
        //    as their dependency cannot be (fully) included.
        //    We do this in a loop because the blocker might have been inordinately large and
        //    we might have to do it multiple times to satisfy tail packing
        'tail_loop: while gas_limit >= MIN_GAS && last < chains.len() {
            // trim if necessary
            if chains[last].gas_limit > gas_limit {
                chains.trim_msgs_at(last, gas_limit, &base_fee);
            }

            // push down if it hasn't been invalidated
            if chains[last].valid {
                for i in last..chains.len() - 1 {
                    if chains[i].cmp_effective(&chains[i + 1]) == Ordering::Greater {
                        break;
                    }
                }

                chains.key_vec.swap(i, i + 1);
            }

            // select the next (valid and fitting) chain and its dependencies for inclusion
            let lst = last; // to make clippy happy, see: https://rust-lang.github.io/rust-clippy/master/index.html#mut_range_bound
            for i in lst..chains.len() {
                let chain = &mut chains[i];
                // has the chain been invalidated
                if !chain.valid {
                    continue;
                }

                // has it already been merged?
                if chain.merged {
                    continue;
                }

                // if gasPerf < 0 we have no more profitable chains
                if chain.gas_perf < 0.0 {
                    break 'tail_loop;
                }

                // compute the dependencies that must be merged and the gas limit including deps
                let mut chain_gas_limit = chain.gas_limit;
                let mut dep_gas_limit = 0;
                let mut chain_deps = vec![];
                let mut cur_chain = chains[i].prev;
                while let Some(cur_chn) = cur_chain {
                    chain_deps.push(cur_chn);
                    let node = chains.get(cur_chn).unwrap();
                    chain_gas_limit += node.gas_limit;
                    dep_gas_limit += node.gas_limit;
                    cur_chain = node.prev;
                }

                // does it all fit in a block
                if chain_gas_limit <= gas_limit {
                    // include it together with all dependencies
                    for i in (0..=chain_deps.len()).rev() {
                        if let Some(cur_chain) = chain_deps.get(i) {
                            let node = chains.get_mut(*cur_chain).unwrap();
                            node.merged = true;
                            result.extend(node.msgs.clone());
                        }

                        chains[i].merged = true;
                        result.extend(chains[i].msgs.clone());
                        gas_limit -= chain_gas_limit;
                        continue;
                    }
                }

                // it doesn't all fit; now we have to take into account the dependent chains before
                // making a decision about trimming or invalidating.
                // if the dependencies exceed the gas limit, then we must invalidate the chain
                // as it can never be included.
                // Otherwise we can just trim and continue
                if dep_gas_limit > gas_limit {
                    let key = chains.get_key_at(i);
                    chains.invalidate(key);
                    last += i + 1;
                    continue 'tail_loop;
                }

                // dependencies fit, just trim it
                chains.trim_msgs_at(i, gas_limit - dep_gas_limit, &base_fee);
                last += i;
                continue 'tail_loop;
            }

            // the merge loop ended after processing all the chains and we we probably have still
            // gas to spare; end the loop.
            break;
        }

        // if we have gasLimit to spare, pick some random (non-negative) chains to fill the block
        // we pick randomly so that we minimize the probability of duplication among all miners
        if gas_limit >= MIN_GAS {
            let mut random_count = 0;

            chains.key_vec.shuffle(&mut thread_rng());

            for i in 0..chains.len() {
                if gas_limit < MIN_GAS {
                    break;
                }

                // has it been merged or invalidated?
                if chains[i].merged || !chains[i].valid {
                    continue;
                }

                // is it negative?
                if chains[i].gas_perf < 0.0 {
                    continue;
                }

                // compute the dependencies that must be merged and the gas limit including deps
                let mut chain_gas_limit = chains[i].gas_limit;
                let mut dep_gas_limit = 0;
                let mut chain_deps = vec![];
                let mut cur_chain = chains[i].prev;
                while let Some(cur_chn) = cur_chain {
                    chain_deps.push(cur_chn);
                    let node = chains.get(cur_chn).unwrap();
                    chain_gas_limit += node.gas_limit;
                    dep_gas_limit += node.gas_limit;
                    cur_chain = node.prev;
                }

                // do the deps fit? if the deps won't fit, invalidate the chain
                if dep_gas_limit > gas_limit {
                    let key = chains.get_key_at(i);
                    chains.invalidate(key);
                    continue;
                }

                // do they fit as it? if it doesn't fit, trim to make it fit if possible
                if chain_gas_limit > gas_limit {
                    chains.trim_msgs_at(i, gas_limit - dep_gas_limit, &base_fee);

                    if !chains[i].valid {
                        continue;
                    }
                }

                // include it together with all dependencies
                for i in (0..chain_deps.len()).rev() {
                    let cur_chain = chain_deps[i];
                    let node = chains.get_mut(cur_chain).unwrap();
                    node.merged = true;
                    result.extend(node.msgs.clone());
                    random_count += node.msgs.len();
                }

                chains[i].merged = true;
                result.extend(chains[i].msgs.clone());
                random_count += chains[i].msgs.len();
                gas_limit -= chain_gas_limit;
            }

            if random_count > 0 {
                log::warn!("optimal selection failed to pack a block; picked {} messages with random selection",
                    random_count);
            }
        }

        Ok(result)
    }

    async fn get_pending_messages(&self, cur_ts: &Tipset, ts: &Tipset) -> Result<Pending, Error> {
        let mut result: Pending = HashMap::new();
        let mut in_sync = false;
        if cur_ts.epoch() == ts.epoch() && cur_ts == ts {
            in_sync = true;
        }

        for (a, mset) in self.pending.read().await.iter() {
            if in_sync {
                result.insert(*a, mset.msgs.clone());
            } else {
                let mut mset_copy = HashMap::new();
                for (nonce, m) in mset.msgs.iter() {
                    mset_copy.insert(*nonce, m.clone());
                }
                result.insert(*a, mset_copy);
            }
        }

        if in_sync {
            return Ok(result);
        }

        // Run head change to do reorg detection
        run_head_change(
            &self.api,
            &self.pending,
            cur_ts.clone(),
            ts.clone(),
            &mut result,
        )
        .await?;

        Ok(result)
    }

    async fn select_priority_messages(
        &self,
        pending: &mut Pending,
        base_fee: &BigInt,
        ts: &Tipset,
    ) -> Result<(Vec<SignedMessage>, i64), Error> {
        let result = Vec::with_capacity(self.config.size_limit_low() as usize);
        let gas_limit = types::BLOCK_GAS_LIMIT;
        let min_gas = 1298450;

        // 1. Get priority actor chains
        let priority = self.config.priority_addrs();
        let mut chains = Chains::new();
        for actor in priority.iter() {
            // remove actor from pending set as we are processing these messages.
            if let Some(mset) = pending.remove(actor) {
                // create chains for the priority actor
                create_message_chains(&self.api, actor, &mset, base_fee, ts, &mut chains).await?;
            }
        }

        if chains.is_empty() {
            return Ok((Vec::new(), gas_limit));
        }

        Ok(merge_and_trim(
            &mut chains,
            result,
            base_fee,
            gas_limit,
            min_gas,
        ))
    }
}

/// Returns merged and trimmed messages with the gas limit
fn merge_and_trim(
    chains: &mut Chains,
    mut result: Vec<SignedMessage>,
    base_fee: &BigInt,
    gas_limit: i64,
    min_gas: i64,
) -> (Vec<SignedMessage>, i64) {
    let mut gas_limit = gas_limit;
    // 2. Sort the chains
    chains.sort(true);

    let first_chain_gas_perf = chains[0].gas_perf;

    if !chains.is_empty() && first_chain_gas_perf < 0.0 {
        log::warn!(
            "all priority messages in mpool have negative gas performance bestGasPerf: {}",
            first_chain_gas_perf
        );
        return (Vec::new(), gas_limit);
    }

    // 3. Merge chains until the block limit, as long as they have non-negative gas performance
    let mut last = chains.len();
    let chain_len = chains.len();
    for i in 0..chain_len {
        let node = &chains[i];

        if node.gas_perf < 0.0 {
            break;
        }

        if node.gas_limit <= gas_limit {
            gas_limit -= node.gas_limit;
            result.extend(node.msgs.clone());
            continue;
        }
        last = i;
        break;
    }

    'tail_loop: while gas_limit >= min_gas && last < chain_len {
        // trim, discard negative performing messages
        chains.trim_msgs_at(last, gas_limit, base_fee);

        // push down if it hasn't been invalidated
        let node = &chains[last];
        if node.valid {
            for i in last..chain_len - 1 {
                // slot_chains
                let cur_node = &chains[i];
                let next_node = &chains[i + 1];
                if cur_node.compare(&next_node) == Ordering::Greater {
                    break;
                }

                chains.key_vec.swap(i, i + 1);
            }
        }

        // select the next (valid and fitting) chain for inclusion
        let lst = last; // to make clippy happy, see: https://rust-lang.github.io/rust-clippy/master/index.html#mut_range_bound
        for i in lst..chains.len() {
            let chain = &mut chains[i];
            if !chain.valid {
                continue;
            }

            // if gas_perf < 0 then we have no more profitable chains
            if chain.gas_perf < 0.0 {
                break 'tail_loop;
            }

            // does it fit in the block?
            if chain.gas_limit <= gas_limit {
                gas_limit -= chain.gas_limit;
                result.append(&mut chain.msgs);
                continue;
            }

            last += i;
            continue 'tail_loop;
        }

        break;
    }

    (result, gas_limit)
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
            add_to_selected_msgs(msg, rmsgs);
        }
    }

    for ts in right_chain {
        for b in ts.blocks() {
            let (msgs, smsgs) = api.read().await.messages_for_block(b)?;

            for msg in smsgs {
                remove_from_selected_msgs(msg.from(), pending, msg.sequence(), rmsgs.borrow_mut())
                    .await?;
            }
            for msg in msgs {
                remove_from_selected_msgs(msg.from(), pending, msg.sequence(), rmsgs.borrow_mut())
                    .await?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod test_selection {
    use super::*;

    use crate::head_change;
    use crate::msgpool::test_provider::{mock_block, TestApi};
    use crate::msgpool::tests::create_smsg;
    use async_std::channel::bounded;
    use async_std::task;
    use crypto::SignatureType;
    use db::MemoryDB;
    use key_management::{KeyStore, KeyStoreConfig, Wallet};
    use message::Message;
    use std::sync::Arc;
    use types::NetworkParams;

    const TEST_GAS_LIMIT: i64 = 6955002;

    fn make_test_mpool() -> MessagePool<TestApi> {
        let tma = TestApi::default();
        task::block_on(async move {
            let (tx, _rx) = bounded(50);
            MessagePool::new(tma, "mptest".to_string(), tx, Default::default()).await
        })
        .unwrap()
    }

    #[async_std::test]
    async fn basic_message_selection() {
        let mpool = make_test_mpool();

        let ks1 = KeyStore::new(KeyStoreConfig::Memory).unwrap();
        let mut w1 = Wallet::new(ks1);
        let a1 = w1.generate_addr(SignatureType::Secp256k1).unwrap();

        let ks2 = KeyStore::new(KeyStoreConfig::Memory).unwrap();
        let mut w2 = Wallet::new(ks2);
        let a2 = w2.generate_addr(SignatureType::Secp256k1).unwrap();

        let b1 = mock_block(1, 1);
        let ts = Tipset::new(vec![b1.clone()]).unwrap();
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
            vec![Tipset::new(vec![b1]).unwrap()],
        )
        .await
        .unwrap();

        // let gas_limit = 6955002;
        api.write()
            .await
            .set_state_balance_raw(&a1, types::DefaultNetworkParams::from_fil(1));
        api.write()
            .await
            .set_state_balance_raw(&a2, types::DefaultNetworkParams::from_fil(1));

        // we create 10 messages from each actor to another, with the first actor paying higher
        // gas prices than the second; we expect message selection to order his messages first
        for i in 0..10 {
            let m = create_smsg(&a2, &a1, &mut w1, i, TEST_GAS_LIMIT, 2 * i + 1);
            mpool.add(m).await.unwrap();
        }
        for i in 0..10 {
            let m = create_smsg(&a1, &a2, &mut w2, i, TEST_GAS_LIMIT, i + 1);
            mpool.add(m).await.unwrap();
        }

        let msgs = mpool.select_messages(&ts, 1.0).await.unwrap();

        assert_eq!(msgs.len(), 20, "Expected 20 messages, got {}", msgs.len());

        let mut next_nonce = 0;
        for i in 0..10 {
            assert_eq!(
                *msgs[i].from(),
                a1,
                "first 10 returned messages should be from actor a1 {}",
                i
            );
            assert_eq!(msgs[i].sequence(), next_nonce, "nonce should be in order");
            next_nonce += 1;
        }

        next_nonce = 0;
        for i in 10..20 {
            assert_eq!(
                *msgs[i].from(),
                a2,
                "next 10 returned messages should be from actor a2 {}",
                i
            );
            assert_eq!(msgs[i].sequence(), next_nonce, "nonce should be in order");
            next_nonce += 1;
        }

        // now we make a block with all the messages and advance the chain
        let b2 = mpool.api.write().await.next_block();
        mpool.api.write().await.set_block_messages(&b2, msgs);
        head_change(
            api.as_ref(),
            bls_sig_cache.as_ref(),
            repub_trigger.clone(),
            republished.as_ref(),
            pending.as_ref(),
            cur_tipset.as_ref(),
            Vec::new(),
            vec![Tipset::new(vec![b2]).unwrap()],
        )
        .await
        .unwrap();

        // we should now have no pending messages in the MessagePool
        // let pending = mpool.pending.read().await;
        assert!(
            mpool.pending.read().await.is_empty(),
            "Expected no pending messages, but got {}",
            mpool.pending.read().await.len()
        );

        // create a block and advance the chain without applying to the mpool
        let mut msgs = Vec::with_capacity(20);
        for i in 10..20 {
            msgs.push(create_smsg(&a2, &a1, &mut w1, i, TEST_GAS_LIMIT, 2 * i + 1));
            msgs.push(create_smsg(&a1, &a2, &mut w2, i, TEST_GAS_LIMIT, i + 1));
        }
        let b3 = mpool.api.write().await.next_block();
        let ts3 = Tipset::new(vec![b3.clone()]).unwrap();
        mpool.api.write().await.set_block_messages(&b3, msgs);

        // now create another set of messages and add them to the mpool
        for i in 20..30 {
            mpool
                .add(create_smsg(
                    &a2,
                    &a1,
                    &mut w1,
                    i,
                    TEST_GAS_LIMIT,
                    2 * i + 200,
                ))
                .await
                .unwrap();
            mpool
                .add(create_smsg(&a1, &a2, &mut w2, i, TEST_GAS_LIMIT, i + 1))
                .await
                .unwrap();
        }
        // select messages in the last tipset; this should include the missed messages as well as
        // the last messages we added, with the first actor's messages first
        // first we need to update the nonce on the api
        mpool.api.write().await.set_state_sequence(&a1, 10);
        mpool.api.write().await.set_state_sequence(&a2, 10);
        let msgs = mpool.select_messages(&ts3, 1.0).await.unwrap();

        assert_eq!(
            msgs.len(),
            20,
            "Expected 20 messages, but got {}",
            msgs.len()
        );

        let mut next_nonce = 20;
        for i in 0..10 {
            assert_eq!(
                *msgs[i].from(),
                a1,
                "first 10 returned messages should be from actor a1"
            );
            assert_eq!(msgs[i].sequence(), next_nonce, "nonce should be in order");
            next_nonce += 1;
        }
        next_nonce = 20;
        for i in 10..20 {
            assert_eq!(
                *msgs[i].from(),
                a2,
                "next 10 returned messages should be from actor a2"
            );
            assert_eq!(msgs[i].sequence(), next_nonce, "nonce should be in order");
            next_nonce += 1;
        }
    }

    #[async_std::test]
    // #[ignore = "test is incredibly slow"]
    // TODO optimize logic tested in this function
    async fn message_selection_trimming() {
        let mpool = make_test_mpool();

        let ks1 = KeyStore::new(KeyStoreConfig::Memory).unwrap();
        let mut w1 = Wallet::new(ks1);
        let a1 = w1.generate_addr(SignatureType::Secp256k1).unwrap();

        let ks2 = KeyStore::new(KeyStoreConfig::Memory).unwrap();
        let mut w2 = Wallet::new(ks2);
        let a2 = w2.generate_addr(SignatureType::Secp256k1).unwrap();

        let b1 = mock_block(1, 1);
        let ts = Tipset::new(vec![b1.clone()]).unwrap();
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
            vec![Tipset::new(vec![b1]).unwrap()],
        )
        .await
        .unwrap();

        // let gas_limit = 6955002;
        api.write()
            .await
            .set_state_balance_raw(&a1, types::DefaultNetworkParams::from_fil(1));
        api.write()
            .await
            .set_state_balance_raw(&a2, types::DefaultNetworkParams::from_fil(1));

        let nmsgs = (types::BLOCK_GAS_LIMIT / TEST_GAS_LIMIT) + 1;

        // make many small chains for the two actors
        for i in 0..nmsgs {
            let bias = (nmsgs - i) / 3;
            let m = create_smsg(
                &a2,
                &a1,
                &mut w1,
                i as u64,
                TEST_GAS_LIMIT,
                (1 + i % 3 + bias) as u64,
            );
            mpool.add(m).await.unwrap();
            let m = create_smsg(
                &a1,
                &a2,
                &mut w2,
                i as u64,
                TEST_GAS_LIMIT,
                (1 + i % 3 + bias) as u64,
            );
            mpool.add(m).await.unwrap();
        }

        let msgs = mpool.select_messages(&ts, 1.0).await.unwrap();

        let expected = types::BLOCK_GAS_LIMIT / TEST_GAS_LIMIT;
        assert_eq!(msgs.len(), expected as usize);
        let mut m_gas_lim = 0;
        for m in msgs.iter() {
            m_gas_lim += m.gas_limit();
        }
        assert!(m_gas_lim <= types::BLOCK_GAS_LIMIT);
    }

    #[async_std::test]
    async fn message_selection_priority() {
        let db = MemoryDB::default();

        let mut mpool = make_test_mpool();

        let ks1 = KeyStore::new(KeyStoreConfig::Memory).unwrap();
        let mut w1 = Wallet::new(ks1);
        let a1 = w1.generate_addr(SignatureType::Secp256k1).unwrap();

        let ks2 = KeyStore::new(KeyStoreConfig::Memory).unwrap();
        let mut w2 = Wallet::new(ks2);
        let a2 = w2.generate_addr(SignatureType::Secp256k1).unwrap();

        // set priority addrs to a1
        let mut mpool_cfg = mpool.get_config().clone();
        mpool_cfg.priority_addrs.push(a1);
        mpool.set_config(&db, mpool_cfg).unwrap();

        let b1 = mock_block(1, 1);
        let ts = Tipset::new(vec![b1.clone()]).unwrap();
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
            vec![Tipset::new(vec![b1]).unwrap()],
        )
        .await
        .unwrap();

        // let gas_limit = 6955002;
        api.write()
            .await
            .set_state_balance_raw(&a1, types::DefaultNetworkParams::from_fil(1));
        api.write()
            .await
            .set_state_balance_raw(&a2, types::DefaultNetworkParams::from_fil(1));

        let nmsgs = 10;

        // make many small chains for the two actors
        for i in 0..nmsgs {
            let bias = (nmsgs - i) / 3;
            let m = create_smsg(
                &a2,
                &a1,
                &mut w1,
                i as u64,
                TEST_GAS_LIMIT,
                (1 + i % 3 + bias) as u64,
            );
            mpool.add(m).await.unwrap();
            let m = create_smsg(
                &a1,
                &a2,
                &mut w2,
                i as u64,
                TEST_GAS_LIMIT,
                (1 + i % 3 + bias) as u64,
            );
            mpool.add(m).await.unwrap();
        }

        let msgs = mpool.select_messages(&ts, 1.0).await.unwrap();

        assert_eq!(msgs.len(), 20);

        let mut next_nonce = 0;
        for i in 0..10 {
            assert_eq!(
                *msgs[i].from(),
                a1,
                "first 10 returned messages should be from actor a1"
            );
            assert_eq!(msgs[i].sequence(), next_nonce, "nonce should be in order");
            next_nonce += 1;
        }
        next_nonce = 0;
        for i in 10..20 {
            assert_eq!(
                *msgs[i].from(),
                a2,
                "next 10 returned messages should be from actor a2"
            );
            assert_eq!(msgs[i].sequence(), next_nonce, "nonce should be in order");
            next_nonce += 1;
        }
    }

    #[async_std::test]
    async fn test_optimal_msg_selection1() {
        // this test uses just a single actor sending messages with a low tq
        // the chain depenent merging algorithm should pick messages from the actor
        // from the start
        let mpool = make_test_mpool();

        // create two actors
        let mut w1 = Wallet::new(KeyStore::new(KeyStoreConfig::Memory).unwrap());
        let a1 = w1.generate_addr(SignatureType::Secp256k1).unwrap();
        let mut w2 = Wallet::new(KeyStore::new(KeyStoreConfig::Memory).unwrap());
        let a2 = w2.generate_addr(SignatureType::Secp256k1).unwrap();

        // create a block
        let b1 = mock_block(1, 1);
        // add block to tipset
        let ts = Tipset::new(vec![b1.clone()]).unwrap();

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
            vec![Tipset::new(vec![b1]).unwrap()],
        )
        .await
        .unwrap();

        api.write()
            .await
            .set_state_balance_raw(&a1, types::DefaultNetworkParams::from_fil(1));

        api.write()
            .await
            .set_state_balance_raw(&a2, types::DefaultNetworkParams::from_fil(1));

        let n_msgs = 10 * types::BLOCK_GAS_LIMIT / TEST_GAS_LIMIT;

        // we create 10 messages from each actor to another, with the first actor paying higher
        // gas prices than the second; we expect message selection to order his messages first
        for i in 0..(n_msgs as usize) {
            let bias = (n_msgs as usize - i) / 3;
            let m = create_smsg(
                &a2,
                &a1,
                &mut w1,
                i as u64,
                TEST_GAS_LIMIT,
                (1 + i % 3 + bias) as u64,
            );
            mpool.add(m).await.unwrap();
        }

        let msgs = mpool.select_messages(&ts, 0.25).await.unwrap();

        let expected_msgs = types::BLOCK_GAS_LIMIT / TEST_GAS_LIMIT;

        assert_eq!(msgs.len(), expected_msgs as usize);

        let mut next_nonce = 0u64;
        for m in msgs {
            assert_eq!(m.message().from(), &a1, "Expected message from a1");
            assert_eq!(
                m.message().sequence,
                next_nonce,
                "expected nonce {} but got {}",
                next_nonce,
                m.message().sequence
            );
            next_nonce += 1;
        }
    }

    #[async_std::test]
    async fn test_optimal_msg_selection2() {
        // this test uses two actors sending messages to each other, with the first
        // actor paying (much) higher gas premium than the second.
        // We select with a low ticket quality; the chain depenent merging algorithm should pick
        // messages from the second actor from the start
        let mpool = make_test_mpool();

        // create two actors
        let mut w1 = Wallet::new(KeyStore::new(KeyStoreConfig::Memory).unwrap());
        let a1 = w1.generate_addr(SignatureType::Secp256k1).unwrap();
        let mut w2 = Wallet::new(KeyStore::new(KeyStoreConfig::Memory).unwrap());
        let a2 = w2.generate_addr(SignatureType::Secp256k1).unwrap();

        // create a block
        let b1 = mock_block(1, 1);
        // add block to tipset
        let ts = Tipset::new(vec![b1.clone()]).unwrap();

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
            vec![Tipset::new(vec![b1]).unwrap()],
        )
        .await
        .unwrap();

        api.write()
            .await
            .set_state_balance_raw(&a1, types::DefaultNetworkParams::from_fil(1)); // in FIL

        api.write()
            .await
            .set_state_balance_raw(&a2, types::DefaultNetworkParams::from_fil(1)); // in FIL

        let n_msgs = 5 * types::BLOCK_GAS_LIMIT / TEST_GAS_LIMIT;
        for i in 0..n_msgs as usize {
            let bias = (n_msgs as usize - i) / 3;
            let m = create_smsg(
                &a2,
                &a1,
                &mut w1,
                i as u64,
                TEST_GAS_LIMIT,
                (200000 + i % 3 + bias) as u64,
            );
            mpool.add(m).await.unwrap();
            let m = create_smsg(
                &a1,
                &a2,
                &mut w2,
                i as u64,
                TEST_GAS_LIMIT,
                (190000 + i % 3 + bias) as u64,
            );
            mpool.add(m).await.unwrap();
        }

        let msgs = mpool.select_messages(&ts, 0.1).await.unwrap();

        let expected_msgs = types::BLOCK_GAS_LIMIT / TEST_GAS_LIMIT;
        assert_eq!(
            msgs.len(),
            expected_msgs as usize,
            "Expected {} messages, but got {}",
            expected_msgs,
            msgs.len()
        );

        let mut n_from1 = 0;
        let mut n_from2 = 0;
        let mut next_nonce1 = 0;
        let mut next_nonce2 = 0;

        for m in msgs {
            if m.message.from() == &a1 {
                if m.message.sequence != next_nonce1 {
                    panic!(
                        "Expected nonce {}, but got {}",
                        next_nonce1, m.message.sequence
                    );
                }
                next_nonce1 += 1;
                n_from1 += 1;
            } else {
                if m.message.sequence != next_nonce2 {
                    panic!(
                        "Expected nonce {}, but got {}",
                        next_nonce2, m.message.sequence
                    );
                }
                next_nonce2 += 1;
                n_from2 += 1;
            }
        }

        dbg!(n_from1, n_from2);

        if n_from1 > n_from2 {
            panic!("Expected more msgs from a2 than a1");
        }
    }

    #[async_std::test]
    async fn test_optimal_message_selection3() {
        // this test uses 10 actors sending a block of messages to each other, with the the first
        // actors paying higher gas premium than the subsequent actors.
        // We select with a low ticket quality; the chain depenent merging algorithm should pick
        // messages from the median actor from the start
        let mpool = make_test_mpool();

        let n_actors = 10;

        let mut actors = vec![];
        let mut wallets = vec![];

        for _ in 0..n_actors {
            let mut wallet = Wallet::new(KeyStore::new(KeyStoreConfig::Memory).unwrap());
            let actor = wallet.generate_addr(SignatureType::Secp256k1).unwrap();

            actors.push(actor);
            wallets.push(wallet);
        }

        // create a block
        let block = mock_block(1, 1);
        // add block to tipset
        let ts = Tipset::new(vec![block.clone()]).unwrap();

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
            vec![Tipset::new(vec![block]).unwrap()],
        )
        .await
        .unwrap();

        for a in &mut actors {
            api.write()
                .await
                .set_state_balance_raw(&a, types::DefaultNetworkParams::from_fil(1));
            // in FIL
        }

        let n_msgs = 1 + types::BLOCK_GAS_LIMIT / TEST_GAS_LIMIT;
        for i in 0..n_msgs {
            for j in 0..n_actors {
                let premium =
                    500000 + 10000 * (n_actors - j) + (n_msgs + 2 - i) / (30 * n_actors) + i % 3;
                let m = create_smsg(
                    &actors[(j % n_actors) as usize],
                    &actors[j as usize],
                    &mut wallets[j as usize],
                    i as u64,
                    TEST_GAS_LIMIT,
                    premium as u64,
                );
                mpool.add(m).await.unwrap();
            }
        }

        let msgs = mpool.select_messages(&ts, 0.1).await.unwrap();
        let expected_msgs = types::BLOCK_GAS_LIMIT / TEST_GAS_LIMIT;

        assert_eq!(
            msgs.len(),
            expected_msgs as usize,
            "Expected {} messages, but got {}",
            expected_msgs,
            msgs.len()
        );

        let who_is = |addr| -> usize {
            for (i, a) in actors.iter().enumerate() {
                if a == addr {
                    return i;
                }
            }
            // Lotus has -1, but since we don't have -ve indexes, set it some unrealistic number
            return 9999999;
        };

        let mut nonces = vec![0; n_actors as usize];
        for m in &msgs {
            let who = who_is(m.message.from()) as usize;
            if who < 3 {
                panic!("got message from {}th actor", who);
            }

            let next_nonce: u64 = nonces[who];
            if m.message.sequence != next_nonce {
                panic!(
                    "expected nonce {} but got {}",
                    next_nonce, m.message.sequence
                );
            }
            nonces[who] += 1;
        }
    }
}
