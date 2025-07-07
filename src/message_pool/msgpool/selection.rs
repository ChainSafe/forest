// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Contains routines for message selection APIs.
//! Whenever a miner is ready to create a block for a tipset, it invokes the
//! `select_messages` API which selects an appropriate set of messages such that
//! it optimizes miner reward and chain capacity. See <https://docs.filecoin.io/mine/lotus/message-pool/#message-selection> for more details

use std::{borrow::BorrowMut, cmp::Ordering, sync::Arc};

use crate::blocks::{BLOCK_MESSAGE_LIMIT, Tipset};
use crate::message::{Message, SignedMessage};
use crate::message_pool::msg_chain::MsgChainNode;
use crate::shim::crypto::SignatureType;
use crate::shim::{address::Address, econ::TokenAmount};
use ahash::{HashMap, HashMapExt};
use anyhow::{Context, bail, ensure};
use parking_lot::RwLock;
use rand::prelude::SliceRandom;
use tracing::{debug, error, warn};

use super::{msg_pool::MessagePool, provider::Provider};
use crate::message_pool::{
    Error, add_to_selected_msgs,
    msg_chain::{Chains, NodeKey, create_message_chains},
    msg_pool::MsgSet,
    msgpool::MIN_GAS,
    remove_from_selected_msgs,
};

type Pending = HashMap<Address, HashMap<u64, SignedMessage>>;

// A cap on maximum number of message to include in a block
const MAX_BLOCK_MSGS: usize = 16000;
const MAX_BLOCKS: usize = 15;
const CBOR_GEN_LIMIT: usize = 8192; // Same limits as in Go's `cbor-gen`.

/// A structure that holds the selected messages for a block.
/// It tracks the gas limit and the limits for different signature types
/// to ensure that the block does not exceed the limits set by the protocol.
struct SelectedMessages {
    /// The messages selected for inclusion in the block.
    msgs: Vec<SignedMessage>,
    /// The remaining gas limit for the block.
    gas_limit: u64,
    /// The remaining limit for `secp256k1` messages in the block.
    secp_limit: u64,
    /// The remaining limit for `bls` messages in the block.
    bls_limit: u64,
}

impl Default for SelectedMessages {
    fn default() -> Self {
        SelectedMessages {
            msgs: Vec::new(),
            gas_limit: crate::shim::econ::BLOCK_GAS_LIMIT,
            secp_limit: CBOR_GEN_LIMIT as u64,
            bls_limit: CBOR_GEN_LIMIT as u64,
        }
    }
}

impl SelectedMessages {
    fn new(msgs: Vec<SignedMessage>, gas_limit: u64) -> Self {
        SelectedMessages {
            msgs,
            gas_limit,
            ..Default::default()
        }
    }

    /// Returns the number of messages selected for inclusion in the block.
    fn len(&self) -> usize {
        self.msgs.len()
    }

    /// Truncates the selected messages to the specified length.
    fn truncate(&mut self, len: usize) {
        self.msgs.truncate(len);
    }

    /// Reduces the gas limit by the specified amount. It ensures that the gas limit does not
    /// go below zero (which would cause a panic).
    fn reduce_gas_limit(&mut self, gas: u64) {
        self.gas_limit = self.gas_limit.saturating_sub(gas);
    }

    /// Reduces the BLS limit by the specified amount. It ensures that the BLS message limit does not
    /// go below zero (which would cause a panic).
    fn reduce_bls_limit(&mut self, bls: u64) {
        self.bls_limit = self.bls_limit.saturating_sub(bls);
    }

    /// Reduces the `Secp256k1` limit by the specified amount. It ensures that the `Secp256k1` message limit
    /// does not go below zero (which would cause a panic).
    fn reduce_secp_limit(&mut self, secp: u64) {
        self.secp_limit = self.secp_limit.saturating_sub(secp);
    }

    /// Extends the selected messages with the given messages.
    fn extend(&mut self, msgs: Vec<SignedMessage>) {
        self.msgs.extend(msgs);
    }

    /// Tries to add a message chain to the selected messages. Returns an error if the chain can't
    /// be added due to block constraints.
    fn try_to_add(&mut self, message_chain: MsgChainNode) -> anyhow::Result<()> {
        let msg_chain_len = message_chain.msgs.len();
        ensure!(
            BLOCK_MESSAGE_LIMIT >= msg_chain_len + self.len(),
            "Message chain is too long to fit in the block: {} messages, limit is {BLOCK_MESSAGE_LIMIT}",
            msg_chain_len + self.len(),
        );
        ensure!(
            self.gas_limit >= message_chain.gas_limit,
            "Message chain gas limit is too high: {} gas, limit is {}",
            message_chain.gas_limit,
            self.gas_limit
        );

        match message_chain.sig_type {
            Some(SignatureType::Bls) => {
                ensure!(
                    self.bls_limit >= msg_chain_len as u64,
                    "BLS limit is too low: {msg_chain_len} messages, limit is {}",
                    self.bls_limit
                );

                self.extend(message_chain.msgs);
                self.reduce_bls_limit(msg_chain_len as u64);
                self.reduce_gas_limit(message_chain.gas_limit);
            }
            Some(SignatureType::Secp256k1) | Some(SignatureType::Delegated) => {
                ensure!(
                    self.secp_limit >= msg_chain_len as u64,
                    "Secp256k1 limit is too low: {msg_chain_len} messages, limit is {}",
                    self.secp_limit
                );

                self.extend(message_chain.msgs);
                self.reduce_secp_limit(msg_chain_len as u64);
                self.reduce_gas_limit(message_chain.gas_limit);
            }
            None => {
                // This is a message with no signature type, which is not allowed in the current
                // implementation. This _should_ never happen, but we handle it gracefully.
                warn!("Tried to add a message chain with no signature type");
            }
        }
        Ok(())
    }

    /// Tries to add a message chain with dependencies to the selected messages.
    /// It will trim or invalidate if appropriate.
    fn try_to_add_with_deps(
        &mut self,
        idx: usize,
        chains: &mut Chains,
        base_fee: &TokenAmount,
    ) -> anyhow::Result<()> {
        let message_chain = chains
            .get_mut_at(idx)
            .context("Couldn't find required message chain")?;
        // compute the dependencies that must be merged and the gas limit including deps
        let mut chain_gas_limit = message_chain.gas_limit;
        let mut chain_msg_limit = message_chain.msgs.len();
        let mut dep_gas_limit = 0;
        let mut dep_message_limit = 0;
        let selected_messages_limit = match message_chain.sig_type {
            Some(SignatureType::Bls) => self.bls_limit as usize,
            Some(SignatureType::Secp256k1) | Some(SignatureType::Delegated) => {
                self.secp_limit as usize
            }
            None => {
                // This is a message with no signature type, which is not allowed in the current
                // implementation. This _should_ never happen, but we handle it gracefully.
                bail!("Tried to add a message chain with no signature type");
            }
        };
        let selected_messages_limit =
            selected_messages_limit.min(BLOCK_MESSAGE_LIMIT.saturating_sub(self.len()));

        let mut chain_deps = vec![];
        let mut cur_chain = message_chain.prev;
        let _ = message_chain; // drop the mutable borrow to avoid conflicts
        while let Some(cur_chn) = cur_chain {
            let node = chains.get(cur_chn).unwrap();
            if !node.merged {
                chain_deps.push(cur_chn);
                chain_gas_limit += node.gas_limit;
                chain_msg_limit += node.msgs.len();
                dep_gas_limit += node.gas_limit;
                dep_message_limit += node.msgs.len();
                cur_chain = node.prev;
            } else {
                break;
            }
        }

        // the chain doesn't fit as-is, so trim / invalidate it and return false
        if chain_gas_limit > self.gas_limit || chain_msg_limit > selected_messages_limit {
            // it doesn't all fit; now we have to take into account the dependent chains before
            // making a decision about trimming or invalidating.
            // if the dependencies exceed the block limits, then we must invalidate the chain
            // as it can never be included.
            // Otherwise we can just trim and continue
            if dep_gas_limit > self.gas_limit || dep_message_limit >= selected_messages_limit {
                chains.invalidate(chains.get_key_at(idx));
            } else {
                // dependencies fit, just trim it
                chains.trim_msgs_at(
                    idx,
                    self.gas_limit.saturating_sub(dep_gas_limit),
                    selected_messages_limit.saturating_sub(dep_message_limit),
                    base_fee,
                );
            }

            bail!("Chain doesn't fit in the block");
        }
        for dep in chain_deps.iter().rev() {
            let cur_chain = chains.get_mut(*dep);
            if let Some(node) = cur_chain {
                node.merged = true;
                self.extend(node.msgs.clone());
            } else {
                bail!("Couldn't find required dependent message chain");
            }
        }

        let message_chain = chains
            .get_mut_at(idx)
            .context("Couldn't find required message chain")?;
        message_chain.merged = true;
        self.extend(message_chain.msgs.clone());
        self.reduce_gas_limit(chain_gas_limit);

        match message_chain.sig_type {
            Some(SignatureType::Bls) => {
                self.reduce_bls_limit(chain_msg_limit as u64);
            }
            Some(SignatureType::Secp256k1) | Some(SignatureType::Delegated) => {
                self.reduce_secp_limit(chain_msg_limit as u64);
            }
            None => {
                // This is a message with no signature type, which is not allowed in the current
                // implementation. This _should_ never happen, but we handle it gracefully.
                bail!("Tried to add a message chain with no signature type");
            }
        }

        Ok(())
    }

    fn trim_chain_at(&mut self, chains: &mut Chains, idx: usize, base_fee: &TokenAmount) {
        let message_chain = match chains.get_at(idx) {
            Some(message_chain) => message_chain,
            None => {
                error!("Tried to trim a message chain that doesn't exist");
                return;
            }
        };
        let msg_limit = BLOCK_MESSAGE_LIMIT.saturating_sub(self.len());
        let msg_limit = match message_chain.sig_type {
            Some(SignatureType::Bls) => std::cmp::min(self.bls_limit, msg_limit as u64),
            Some(SignatureType::Secp256k1) | Some(SignatureType::Delegated) => {
                std::cmp::min(self.secp_limit, msg_limit as u64)
            }
            _ => {
                // This is a message with no signature type, which is not allowed in the current
                // implementation. This _should_ never happen, but we handle it gracefully.
                error!("Tried to trim a message chain with no signature type");
                return;
            }
        };

        if message_chain.gas_limit > self.gas_limit || message_chain.msgs.len() > msg_limit as usize
        {
            chains.trim_msgs_at(idx, self.gas_limit, msg_limit as usize, base_fee);
        }
    }
}

impl<T> MessagePool<T>
where
    T: Provider,
{
    /// Forest employs a sophisticated algorithm for selecting messages
    /// for inclusion from the pool, given the ticket quality of a miner.
    /// This method selects messages for including in a block.
    pub fn select_messages(&self, ts: &Tipset, tq: f64) -> Result<Vec<SignedMessage>, Error> {
        let cur_ts = self.cur_tipset.lock().clone();
        // if the ticket quality is high enough that the first block has higher
        // probability than any other block, then we don't bother with optimal
        // selection because the first block will always have higher effective
        // performance. Otherwise we select message optimally based on effective
        // performance of chains.
        let mut msgs = if tq > 0.84 {
            self.select_messages_greedy(&cur_ts, ts)
        } else {
            self.select_messages_optimal(&cur_ts, ts, tq)
        }?;

        if msgs.len() > MAX_BLOCK_MSGS {
            warn!(
                "Message selection chose too many messages: {} > {MAX_BLOCK_MSGS}",
                msgs.len(),
            );
            msgs.truncate(MAX_BLOCK_MSGS)
        }

        Ok(msgs.msgs)
    }

    fn select_messages_greedy(
        &self,
        cur_ts: &Tipset,
        ts: &Tipset,
    ) -> Result<SelectedMessages, Error> {
        let base_fee = self.api.chain_compute_base_fee(ts)?;

        // 0. Load messages from the target tipset; if it is the same as the current
        // tipset in    the mpool, then this is just the pending messages
        let mut pending = self.get_pending_messages(cur_ts, ts)?;

        if pending.is_empty() {
            return Ok(SelectedMessages::default());
        }

        // 0b. Select all priority messages that fit in the block
        let selected_msgs = self.select_priority_messages(&mut pending, &base_fee, ts)?;

        // check if block has been filled
        if selected_msgs.gas_limit < MIN_GAS || selected_msgs.len() >= BLOCK_MESSAGE_LIMIT {
            return Ok(selected_msgs);
        }

        // 1. Create a list of dependent message chains with maximal gas reward per
        // limit consumed
        let mut chains = Chains::new();
        for (actor, mset) in pending.into_iter() {
            create_message_chains(
                self.api.as_ref(),
                &actor,
                &mset,
                &base_fee,
                ts,
                &mut chains,
                &self.chain_config,
            )?;
        }

        Ok(merge_and_trim(
            &mut chains,
            selected_msgs,
            &base_fee,
            MIN_GAS,
        ))
    }

    #[allow(clippy::indexing_slicing)]
    fn select_messages_optimal(
        &self,
        cur_ts: &Tipset,
        target_tipset: &Tipset,
        ticket_quality: f64,
    ) -> Result<SelectedMessages, Error> {
        let base_fee = self.api.chain_compute_base_fee(target_tipset)?;

        // 0. Load messages from the target tipset; if it is the same as the current
        // tipset in    the mpool, then this is just the pending messages
        let mut pending = self.get_pending_messages(cur_ts, target_tipset)?;

        if pending.is_empty() {
            return Ok(SelectedMessages::default());
        }

        // 0b. Select all priority messages that fit in the block
        let mut selected_msgs =
            self.select_priority_messages(&mut pending, &base_fee, target_tipset)?;

        // check if block has been filled
        if selected_msgs.gas_limit < MIN_GAS || selected_msgs.len() >= BLOCK_MESSAGE_LIMIT {
            return Ok(selected_msgs);
        }

        // 1. Create a list of dependent message chains with maximal gas reward per
        // limit consumed
        let mut chains = Chains::new();
        for (actor, mset) in pending.into_iter() {
            create_message_chains(
                self.api.as_ref(),
                &actor,
                &mset,
                &base_fee,
                target_tipset,
                &mut chains,
                &self.chain_config,
            )?;
        }

        // 2. Sort the chains
        chains.sort(false);

        if chains.get_at(0).is_some_and(|it| it.gas_perf < 0.0) {
            tracing::warn!(
                "all messages in mpool have non-positive gas performance {}",
                chains[0].gas_perf
            );
            return Ok(selected_msgs);
        }

        // 3. Partition chains into blocks (without trimming)
        //    we use the full block_gas_limit (as opposed to the residual `gas_limit`
        //    from the priority message selection) as we have to account for
        //    what other block providers are doing
        let mut next_chain = 0;
        let mut partitions: Vec<Vec<NodeKey>> = vec![vec![]; MAX_BLOCKS];
        let mut i = 0;
        while i < MAX_BLOCKS && next_chain < chains.len() {
            let mut gas_limit = crate::shim::econ::BLOCK_GAS_LIMIT;
            let mut msg_limit = BLOCK_MESSAGE_LIMIT;
            while next_chain < chains.len() {
                let chain_key = chains.key_vec[next_chain];
                next_chain += 1;
                partitions[i].push(chain_key);
                let chain = chains.get(chain_key).unwrap();
                let chain_gas_limit = chain.gas_limit;
                if gas_limit < chain_gas_limit {
                    break;
                }
                gas_limit = gas_limit.saturating_sub(chain_gas_limit);
                msg_limit = msg_limit.saturating_sub(chain.msgs.len());
                if gas_limit < MIN_GAS || msg_limit == 0 {
                    break;
                }
            }
            i += 1;
        }

        // 4. Compute effective performance for each chain, based on the partition they
        // fall into    The effective performance is the gas_perf of the chain *
        // block probability
        let block_prob = crate::message_pool::block_probabilities(ticket_quality);
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

        // 6. Merge the head chains to produce the list of messages selected for
        //    inclusion subject to the residual block limits
        //    When a chain is merged in, all its previous dependent chains *must* also
        //    be merged in or we'll have a broken block
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

            match selected_msgs.try_to_add_with_deps(i, &mut chains, &base_fee) {
                Ok(_) => {
                    // adjust the effective performance for all subsequent chains
                    if let Some(next_key) = chains[i].next {
                        let next_node = chains.get_mut(next_key).unwrap();
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

                    // re-sort to account for already merged chains and effective performance
                    // adjustments the sort *must* be stable or we end up getting
                    // negative gasPerfs pushed up.
                    chains.sort_range_effective(i + 1..);

                    continue;
                }
                Err(e) => {
                    debug!("Failed to add message chain with dependencies: {e}");
                }
            }

            // we can't fit this chain and its dependencies because of block gasLimit -- we
            // are at the edge
            last = i;
            break;
        }

        // 7. We have reached the edge of what can fit wholesale; if we still hae
        // available    gasLimit to pack some more chains, then trim the last
        // chain and push it down.
        //
        // Trimming invalidates subsequent dependent chains so that they can't be selected
        // as their dependency cannot be (fully) included. We do this in a loop because the blocker
        // might have been inordinately large and we might have to do it
        // multiple times to satisfy tail packing.
        'tail_loop: while selected_msgs.gas_limit >= MIN_GAS && last < chains.len() {
            if !chains[last].valid {
                // the chain has been invalidated, we can't use it
                last += 1;
                continue;
            }

            // trim if necessary
            selected_msgs.trim_chain_at(&mut chains, last, &base_fee);

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

                match selected_msgs.try_to_add_with_deps(i, &mut chains, &base_fee) {
                    Ok(_) => continue,
                    Err(e) => debug!("Failed to add message chain with dependencies: {e}"),
                }

                continue 'tail_loop;
            }

            // the merge loop ended after processing all the chains and we we probably have
            // still gas to spare; end the loop.
            break;
        }

        // if we have room to spare, pick some random (non-negative) chains to fill
        // the block
        // we pick randomly so that we minimize the probability of
        // duplication among all block producers
        if selected_msgs.gas_limit >= MIN_GAS && selected_msgs.msgs.len() <= BLOCK_MESSAGE_LIMIT {
            let pre_random_length = selected_msgs.len();

            chains
                .key_vec
                .shuffle(&mut crate::utils::rand::forest_rng());

            for i in 0..chains.len() {
                if selected_msgs.gas_limit < MIN_GAS || selected_msgs.len() >= BLOCK_MESSAGE_LIMIT {
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

                if selected_msgs
                    .try_to_add_with_deps(i, &mut chains, &base_fee)
                    .is_ok()
                {
                    // we added it, continue
                    continue;
                }

                if chains[i].valid {
                    // chain got trimmer on the previous call to `try_to_add_with_deps`, so it can
                    // now be included.
                    selected_msgs
                        .try_to_add_with_deps(i, &mut chains, &base_fee)
                        .context("Failed to add message chain with dependencies")?;
                    continue;
                }
            }

            if selected_msgs.len() != pre_random_length {
                tracing::warn!(
                    "optimal selection failed to pack a block; picked {} messages with random selection",
                    selected_msgs.len() - pre_random_length
                );
            }
        }

        Ok(selected_msgs)
    }

    fn get_pending_messages(&self, cur_ts: &Tipset, ts: &Tipset) -> Result<Pending, Error> {
        let mut result: Pending = HashMap::new();
        let mut in_sync = false;
        if cur_ts.epoch() == ts.epoch() && cur_ts == ts {
            in_sync = true;
        }

        for (a, mset) in self.pending.read().iter() {
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
            self.api.as_ref(),
            &self.pending,
            cur_ts.clone(),
            ts.clone(),
            &mut result,
        )?;

        Ok(result)
    }

    fn select_priority_messages(
        &self,
        pending: &mut Pending,
        base_fee: &TokenAmount,
        ts: &Tipset,
    ) -> Result<SelectedMessages, Error> {
        let result = Vec::with_capacity(self.config.size_limit_low() as usize);
        let gas_limit = crate::shim::econ::BLOCK_GAS_LIMIT;
        let min_gas = 1298450;

        // 1. Get priority actor chains
        let priority = self.config.priority_addrs();
        let mut chains = Chains::new();
        for actor in priority.iter() {
            // remove actor from pending set as we are processing these messages.
            if let Some(mset) = pending.remove(actor) {
                // create chains for the priority actor
                create_message_chains(
                    self.api.as_ref(),
                    actor,
                    &mset,
                    base_fee,
                    ts,
                    &mut chains,
                    &self.chain_config,
                )?;
            }
        }

        if chains.is_empty() {
            return Ok(SelectedMessages::new(Vec::new(), gas_limit));
        }

        Ok(merge_and_trim(
            &mut chains,
            SelectedMessages::new(result, gas_limit),
            base_fee,
            min_gas,
        ))
    }
}

/// Returns merged and trimmed messages with the gas limit
#[allow(clippy::indexing_slicing)]
fn merge_and_trim(
    chains: &mut Chains,
    mut selected_msgs: SelectedMessages,
    base_fee: &TokenAmount,
    min_gas: u64,
) -> SelectedMessages {
    if chains.is_empty() {
        return selected_msgs;
    }

    // 2. Sort the chains
    chains.sort(true);

    let first_chain_gas_perf = chains[0].gas_perf;

    if !chains.is_empty() && first_chain_gas_perf < 0.0 {
        warn!(
            "all priority messages in mpool have negative gas performance bestGasPerf: {}",
            first_chain_gas_perf
        );
        return selected_msgs;
    }

    // 3. Merge chains until the block limit, as long as they have non-negative gas
    // performance
    let mut last = chains.len();
    for i in 0..chains.len() {
        let node = &chains[i];

        if node.gas_perf < 0.0 {
            break;
        }

        if selected_msgs.try_to_add(node.clone()).is_ok() {
            // there was room, we added the chain, keep going
            continue;
        }

        // we can't fit this chain because of block gas limit -- we are at the edge
        last = i;
        break;
    }

    'tail_loop: while selected_msgs.gas_limit >= min_gas && last < chains.len() {
        // trim, discard negative performing messages
        selected_msgs.trim_chain_at(chains, last, base_fee);

        // push down if it hasn't been invalidated
        let node = &chains[last];
        if node.valid {
            for i in last..chains.len() - 1 {
                // slot_chains
                let cur_node = &chains[i];
                let next_node = &chains[i + 1];
                if cur_node.compare(next_node) == Ordering::Greater {
                    break;
                }

                chains.key_vec.swap(i, i + 1);
            }
        }

        // select the next (valid and fitting) chain for inclusion
        let lst = last; // to make clippy happy, see: https://rust-lang.github.io/rust-clippy/master/index.html#mut_range_bound
        for i in lst..chains.len() {
            let chain = chains[i].clone();
            if !chain.valid {
                continue;
            }

            // if gas_perf < 0 then we have no more profitable chains
            if chain.gas_perf < 0.0 {
                break 'tail_loop;
            }

            // does it fit in the block?
            if selected_msgs.try_to_add(chain).is_ok() {
                // there was room, we added the chain, keep going
                continue;
            }

            // this chain needs to be trimmed
            last += i;
            continue 'tail_loop;
        }

        break;
    }

    selected_msgs
}

/// Like `head_change`, except it doesn't change the state of the `MessagePool`.
/// It simulates a head change call.
// This logic should probably be implemented in the ChainStore. It handles
// reorgs.
pub(in crate::message_pool) fn run_head_change<T>(
    api: &T,
    pending: &RwLock<HashMap<Address, MsgSet>>,
    from: Tipset,
    to: Tipset,
    rmsgs: &mut HashMap<Address, HashMap<u64, SignedMessage>>,
) -> Result<(), Error>
where
    T: Provider,
{
    let mut left = Arc::new(from);
    let mut right = Arc::new(to);
    let mut left_chain = Vec::new();
    let mut right_chain = Vec::new();
    while left != right {
        if left.epoch() > right.epoch() {
            left_chain.push(left.as_ref().clone());
            let par = api.load_tipset(left.parents())?;
            left = par;
        } else {
            right_chain.push(right.as_ref().clone());
            let par = api.load_tipset(right.parents())?;
            right = par;
        }
    }
    for ts in left_chain {
        let mut msgs: Vec<SignedMessage> = Vec::new();
        for block in ts.block_headers() {
            let (_, smsgs) = api.messages_for_block(block)?;
            msgs.extend(smsgs);
        }
        for msg in msgs {
            add_to_selected_msgs(msg, rmsgs);
        }
    }

    for ts in right_chain {
        for b in ts.block_headers() {
            let (msgs, smsgs) = api.messages_for_block(b)?;

            for msg in smsgs {
                remove_from_selected_msgs(
                    &msg.from(),
                    pending,
                    msg.sequence(),
                    rmsgs.borrow_mut(),
                )?;
            }
            for msg in msgs {
                remove_from_selected_msgs(&msg.from, pending, msg.sequence, rmsgs.borrow_mut())?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod test_selection {
    use std::sync::Arc;

    use crate::db::MemoryDB;
    use crate::key_management::{KeyStore, KeyStoreConfig, Wallet};
    use crate::message::Message;
    use crate::shim::crypto::SignatureType;
    use crate::shim::econ::BLOCK_GAS_LIMIT;
    use tokio::task::JoinSet;

    use super::*;
    use crate::message_pool::{
        head_change,
        msgpool::{
            test_provider::{TestApi, mock_block},
            tests::{create_fake_smsg, create_smsg},
        },
    };

    const TEST_GAS_LIMIT: i64 = 6955002;

    fn make_test_mpool(joinset: &mut JoinSet<anyhow::Result<()>>) -> MessagePool<TestApi> {
        let tma = TestApi::default();
        let (tx, _rx) = flume::bounded(50);
        MessagePool::new(tma, tx, Default::default(), Arc::default(), joinset).unwrap()
    }

    /// Creates a a tipset with a mocked block and performs a head change to setup the
    /// [`MessagePool`] for testing.
    async fn mock_tipset(mpool: &mut MessagePool<TestApi>) -> Tipset {
        let b1 = mock_block(1, 1);
        let ts = Tipset::from(&b1);
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
            vec![Tipset::from(b1)],
        )
        .await
        .unwrap();

        ts
    }

    #[tokio::test]
    async fn basic_message_selection() {
        let mut joinset = JoinSet::new();
        let mpool = make_test_mpool(&mut joinset);

        let ks1 = KeyStore::new(KeyStoreConfig::Memory).unwrap();
        let mut w1 = Wallet::new(ks1);
        let a1 = w1.generate_addr(SignatureType::Secp256k1).unwrap();

        let ks2 = KeyStore::new(KeyStoreConfig::Memory).unwrap();
        let mut w2 = Wallet::new(ks2);
        let a2 = w2.generate_addr(SignatureType::Secp256k1).unwrap();

        let b1 = mock_block(1, 1);
        let ts = Tipset::from(&b1);
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
            vec![Tipset::from(b1)],
        )
        .await
        .unwrap();

        api.set_state_balance_raw(&a1, TokenAmount::from_whole(1));
        api.set_state_balance_raw(&a2, TokenAmount::from_whole(1));

        // we create 10 messages from each actor to another, with the first actor paying
        // higher gas prices than the second; we expect message selection to
        // order his messages first
        for i in 0..10 {
            let m = create_smsg(&a2, &a1, &mut w1, i, TEST_GAS_LIMIT, 2 * i + 1);
            mpool.add(m).unwrap();
        }
        for i in 0..10 {
            let m = create_smsg(&a1, &a2, &mut w2, i, TEST_GAS_LIMIT, i + 1);
            mpool.add(m).unwrap();
        }

        let msgs = mpool.select_messages(&ts, 1.0).unwrap();

        assert_eq!(msgs.len(), 20, "Expected 20 messages, got {}", msgs.len());

        let mut next_nonce = 0;
        for (i, msg) in msgs.iter().enumerate().take(10) {
            assert_eq!(
                msg.from(),
                a1,
                "first 10 returned messages should be from actor a1 {i}",
            );
            assert_eq!(msg.sequence(), next_nonce, "nonce should be in order");
            next_nonce += 1;
        }

        next_nonce = 0;
        for (i, msg) in msgs.iter().enumerate().take(20).skip(10) {
            assert_eq!(
                msg.from(),
                a2,
                "next 10 returned messages should be from actor a2 {i}",
            );
            assert_eq!(msg.sequence(), next_nonce, "nonce should be in order");
            next_nonce += 1;
        }

        // now we make a block with all the messages and advance the chain
        let b2 = mpool.api.next_block();
        mpool.api.set_block_messages(&b2, msgs);
        head_change(
            api.as_ref(),
            bls_sig_cache.as_ref(),
            repub_trigger.clone(),
            republished.as_ref(),
            pending.as_ref(),
            cur_tipset.as_ref(),
            Vec::new(),
            vec![Tipset::from(b2)],
        )
        .await
        .unwrap();

        // we should now have no pending messages in the MessagePool
        // let pending = mpool.pending.read().await;
        assert!(
            mpool.pending.read().is_empty(),
            "Expected no pending messages, but got {}",
            mpool.pending.read().len()
        );

        // create a block and advance the chain without applying to the mpool
        let mut msgs = Vec::with_capacity(20);
        for i in 10..20 {
            msgs.push(create_smsg(&a2, &a1, &mut w1, i, TEST_GAS_LIMIT, 2 * i + 1));
            msgs.push(create_smsg(&a1, &a2, &mut w2, i, TEST_GAS_LIMIT, i + 1));
        }
        let b3 = mpool.api.next_block();
        let ts3 = Tipset::from(&b3);
        mpool.api.set_block_messages(&b3, msgs);

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
                .unwrap();
            mpool
                .add(create_smsg(&a1, &a2, &mut w2, i, TEST_GAS_LIMIT, i + 1))
                .unwrap();
        }
        // select messages in the last tipset; this should include the missed messages
        // as well as the last messages we added, with the first actor's
        // messages first first we need to update the nonce on the api
        mpool.api.set_state_sequence(&a1, 10);
        mpool.api.set_state_sequence(&a2, 10);
        let msgs = mpool.select_messages(&ts3, 1.0).unwrap();

        assert_eq!(
            msgs.len(),
            20,
            "Expected 20 messages, but got {}",
            msgs.len()
        );

        let mut next_nonce = 20;
        for msg in msgs.iter().take(10) {
            assert_eq!(
                msg.from(),
                a1,
                "first 10 returned messages should be from actor a1"
            );
            assert_eq!(msg.sequence(), next_nonce, "nonce should be in order");
            next_nonce += 1;
        }
        next_nonce = 20;
        for msg in msgs.iter().take(20).skip(10) {
            assert_eq!(
                msg.from(),
                a2,
                "next 10 returned messages should be from actor a2"
            );
            assert_eq!(msg.sequence(), next_nonce, "nonce should be in order");
            next_nonce += 1;
        }
    }

    #[tokio::test]
    async fn message_selection_trimming_gas() {
        let mut joinset = JoinSet::new();
        let mut mpool = make_test_mpool(&mut joinset);
        let ts = mock_tipset(&mut mpool).await;
        let api = mpool.api.clone();

        let ks1 = KeyStore::new(KeyStoreConfig::Memory).unwrap();
        let mut w1 = Wallet::new(ks1);
        let a1 = w1.generate_addr(SignatureType::Secp256k1).unwrap();

        let ks2 = KeyStore::new(KeyStoreConfig::Memory).unwrap();
        let mut w2 = Wallet::new(ks2);
        let a2 = w2.generate_addr(SignatureType::Secp256k1).unwrap();

        api.set_state_balance_raw(&a1, TokenAmount::from_whole(1));
        api.set_state_balance_raw(&a2, TokenAmount::from_whole(1));

        let nmsgs = (crate::shim::econ::BLOCK_GAS_LIMIT as i64 / TEST_GAS_LIMIT) + 1;

        // make many small chains for the two actors
        for i in 0..nmsgs {
            let bias = (nmsgs - i) / 3;
            let m = create_fake_smsg(
                &mpool,
                &a2,
                &a1,
                i as u64,
                TEST_GAS_LIMIT,
                (1 + i % 3 + bias) as u64,
            );
            mpool.add(m).unwrap();
            let m = create_fake_smsg(
                &mpool,
                &a1,
                &a2,
                i as u64,
                TEST_GAS_LIMIT,
                (1 + i % 3 + bias) as u64,
            );
            mpool.add(m).unwrap();
        }

        let msgs = mpool.select_messages(&ts, 1.0).unwrap();

        let expected = crate::shim::econ::BLOCK_GAS_LIMIT as i64 / TEST_GAS_LIMIT;
        assert_eq!(msgs.len(), expected as usize);
        let m_gas_limit = msgs.iter().map(|m| m.gas_limit()).sum::<u64>();
        assert!(m_gas_limit <= crate::shim::econ::BLOCK_GAS_LIMIT);
    }

    #[tokio::test]
    async fn message_selection_trimming_msgs_basic() {
        let mut joinset = JoinSet::new();
        let mut mpool = make_test_mpool(&mut joinset);
        let ts = mock_tipset(&mut mpool).await;
        let api = mpool.api.clone();

        let keystore = KeyStore::new(KeyStoreConfig::Memory).unwrap();
        let mut wallet = Wallet::new(keystore);
        let address = wallet.generate_addr(SignatureType::Secp256k1).unwrap();

        api.set_state_balance_raw(&address, TokenAmount::from_whole(1));

        // create a larger than selectable chain
        for i in 0..BLOCK_MESSAGE_LIMIT {
            let msg = create_fake_smsg(&mpool, &address, &address, i as u64, 200_000, 100);
            mpool.add(msg).unwrap();
        }

        let msgs = mpool.select_messages(&ts, 1.0).unwrap();
        assert_eq!(
            msgs.len(),
            CBOR_GEN_LIMIT,
            "Expected {CBOR_GEN_LIMIT} messages, got {}",
            msgs.len()
        );

        // check that the gas limit is not exceeded
        let m_gas_limit = msgs.iter().map(|m| m.gas_limit()).sum::<u64>();
        assert!(
            m_gas_limit <= BLOCK_GAS_LIMIT,
            "Selected messages gas limit {m_gas_limit} exceeds block gas limit {BLOCK_GAS_LIMIT}",
        );
    }

    #[tokio::test]
    async fn message_selection_trimming_msgs_two_senders() {
        let mut joinset = JoinSet::new();
        let mut mpool = make_test_mpool(&mut joinset);
        let ts = mock_tipset(&mut mpool).await;
        let api = mpool.api.clone();

        let keystore_1 = KeyStore::new(KeyStoreConfig::Memory).unwrap();
        let mut wallet_1 = Wallet::new(keystore_1);
        let address_1 = wallet_1.generate_addr(SignatureType::Secp256k1).unwrap();

        let keystore_2 = KeyStore::new(KeyStoreConfig::Memory).unwrap();
        let mut wallet_2 = Wallet::new(keystore_2);
        let address_2 = wallet_2.generate_addr(SignatureType::Bls).unwrap();

        api.set_state_balance_raw(&address_1, TokenAmount::from_whole(1));
        api.set_state_balance_raw(&address_2, TokenAmount::from_whole(1));

        // create 2 larger than selectable chains
        for i in 0..BLOCK_MESSAGE_LIMIT {
            let msg = create_smsg(
                &address_2,
                &address_1,
                &mut wallet_1,
                i as u64,
                300_000,
                100,
            );
            mpool.add(msg).unwrap();
            // higher has price, those should be preferred and fill the block up to
            // the [`CBOR_GEN_LIMIT`] messages.
            let msg = create_smsg(
                &address_1,
                &address_2,
                &mut wallet_2,
                i as u64,
                300_000,
                1000,
            );
            mpool.add(msg).unwrap();
        }
        let msgs = mpool.select_messages(&ts, 1.0).unwrap();
        // check that the gas limit is not exceeded
        let m_gas_limit = msgs.iter().map(|m| m.gas_limit()).sum::<u64>();
        assert!(
            m_gas_limit <= BLOCK_GAS_LIMIT,
            "Selected messages gas limit {m_gas_limit} exceeds block gas limit {BLOCK_GAS_LIMIT}",
        );
        let bls_msgs = msgs.iter().filter(|m| m.is_bls()).count();
        assert_eq!(
            CBOR_GEN_LIMIT, bls_msgs,
            "Expected {CBOR_GEN_LIMIT} bls messages, got {bls_msgs}."
        );
        assert_eq!(
            msgs.len(),
            BLOCK_MESSAGE_LIMIT,
            "Expected {BLOCK_MESSAGE_LIMIT} messages, got {}",
            msgs.len()
        );
    }

    #[tokio::test]
    async fn message_selection_trimming_msgs_two_senders_complex() {
        let mut joinset = JoinSet::new();
        let mut mpool = make_test_mpool(&mut joinset);
        let ts = mock_tipset(&mut mpool).await;
        let api = mpool.api.clone();

        let keystore_1 = KeyStore::new(KeyStoreConfig::Memory).unwrap();
        let mut wallet_1 = Wallet::new(keystore_1);
        let address_1 = wallet_1.generate_addr(SignatureType::Secp256k1).unwrap();

        let keystore_2 = KeyStore::new(KeyStoreConfig::Memory).unwrap();
        let mut wallet_2 = Wallet::new(keystore_2);
        let address_2 = wallet_2.generate_addr(SignatureType::Bls).unwrap();

        api.set_state_balance_raw(&address_1, TokenAmount::from_whole(1));
        api.set_state_balance_raw(&address_2, TokenAmount::from_whole(1));

        // create two almost max-length chains of equal value
        let mut counter = 0;
        for i in 0..CBOR_GEN_LIMIT {
            counter += 1;
            let msg = create_smsg(
                &address_2,
                &address_1,
                &mut wallet_1,
                i as u64,
                300_000,
                100,
            );
            mpool.add(msg).unwrap();
            // higher has price, those should be preferred and fill the block up to
            // the [`CBOR_GEN_LIMIT`] messages.
            let msg = create_smsg(
                &address_1,
                &address_2,
                &mut wallet_2,
                i as u64,
                300_000,
                100,
            );
            mpool.add(msg).unwrap();
        }

        // address_1 8192th message is worth more than address_2 8192th message
        let msg = create_smsg(
            &address_2,
            &address_1,
            &mut wallet_1,
            counter as u64,
            300_000,
            1000,
        );
        mpool.add(msg).unwrap();

        let msg = create_smsg(
            &address_1,
            &address_2,
            &mut wallet_2,
            counter as u64,
            300_000,
            100,
        );
        mpool.add(msg).unwrap();

        counter += 1;

        // address 2 (uneselectable) message is worth so much!
        let msg = create_smsg(
            &address_2,
            &address_1,
            &mut wallet_1,
            counter as u64,
            400_000,
            1_000_000,
        );
        mpool.add(msg).unwrap();

        let msgs = mpool.select_messages(&ts, 1.0).unwrap();
        // check that the gas limit is not exceeded
        let m_gas_limit = msgs.iter().map(|m| m.gas_limit()).sum::<u64>();
        assert!(
            m_gas_limit <= BLOCK_GAS_LIMIT,
            "Selected messages gas limit {m_gas_limit} exceeds block gas limit {BLOCK_GAS_LIMIT}",
        );
        // We should have taken the SECP chain from address_1.
        let secps_len = msgs.iter().filter(|m| m.is_secp256k1()).count();
        assert_eq!(
            CBOR_GEN_LIMIT, secps_len,
            "Expected {CBOR_GEN_LIMIT} secp messages, got {secps_len}."
        );
        // The remaining messages should be BLS messages.
        assert_eq!(
            msgs.len(),
            BLOCK_MESSAGE_LIMIT,
            "Expected {BLOCK_MESSAGE_LIMIT} messages, got {}",
            msgs.len()
        );
    }

    #[tokio::test]
    async fn message_selection_priority() {
        let db = MemoryDB::default();

        let mut joinset = JoinSet::new();
        let mut mpool = make_test_mpool(&mut joinset);
        let ts = mock_tipset(&mut mpool).await;
        let api = mpool.api.clone();

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

        // let gas_limit = 6955002;
        api.set_state_balance_raw(&a1, TokenAmount::from_whole(1));
        api.set_state_balance_raw(&a2, TokenAmount::from_whole(1));

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
            mpool.add(m).unwrap();
            let m = create_smsg(
                &a1,
                &a2,
                &mut w2,
                i as u64,
                TEST_GAS_LIMIT,
                (1 + i % 3 + bias) as u64,
            );
            mpool.add(m).unwrap();
        }

        let msgs = mpool.select_messages(&ts, 1.0).unwrap();

        assert_eq!(msgs.len(), 20);

        let mut next_nonce = 0;
        for msg in msgs.iter().take(10) {
            assert_eq!(
                msg.from(),
                a1,
                "first 10 returned messages should be from actor a1"
            );
            assert_eq!(msg.sequence(), next_nonce, "nonce should be in order");
            next_nonce += 1;
        }
        next_nonce = 0;
        for msg in msgs.iter().take(20).skip(10) {
            assert_eq!(
                msg.from(),
                a2,
                "next 10 returned messages should be from actor a2"
            );
            assert_eq!(msg.sequence(), next_nonce, "nonce should be in order");
            next_nonce += 1;
        }
    }

    #[tokio::test]
    async fn test_optimal_msg_selection1() {
        // this test uses just a single actor sending messages with a low tq
        // the chain dependent merging algorithm should pick messages from the actor
        // from the start
        let mut joinset = JoinSet::new();
        let mut mpool = make_test_mpool(&mut joinset);
        let ts = mock_tipset(&mut mpool).await;
        let api = mpool.api.clone();

        // create two actors
        let mut w1 = Wallet::new(KeyStore::new(KeyStoreConfig::Memory).unwrap());
        let a1 = w1.generate_addr(SignatureType::Secp256k1).unwrap();
        let mut w2 = Wallet::new(KeyStore::new(KeyStoreConfig::Memory).unwrap());
        let a2 = w2.generate_addr(SignatureType::Secp256k1).unwrap();

        api.set_state_balance_raw(&a1, TokenAmount::from_whole(1));
        api.set_state_balance_raw(&a2, TokenAmount::from_whole(1));

        let n_msgs = 10 * crate::shim::econ::BLOCK_GAS_LIMIT as i64 / TEST_GAS_LIMIT;

        // we create n_msgs messages from each actor to another, with the first actor paying
        // higher gas prices than the second; we expect message selection to
        // order his messages first
        for i in 0..(n_msgs as usize) {
            let bias = (n_msgs as usize - i) / 3;
            let m = create_fake_smsg(
                &mpool,
                &a2,
                &a1,
                i as u64,
                TEST_GAS_LIMIT,
                (1 + i % 3 + bias) as u64,
            );
            mpool.add(m).unwrap();
        }

        let msgs = mpool.select_messages(&ts, 0.25).unwrap();

        let expected_msgs = crate::shim::econ::BLOCK_GAS_LIMIT as i64 / TEST_GAS_LIMIT;

        assert_eq!(msgs.len(), expected_msgs as usize);

        for (next_nonce, m) in msgs.into_iter().enumerate() {
            assert_eq!(m.from(), a1, "Expected message from a1");
            assert_eq!(
                m.message().sequence,
                next_nonce as u64,
                "expected nonce {} but got {}",
                next_nonce,
                m.message().sequence
            );
        }
    }

    #[tokio::test]
    async fn test_optimal_msg_selection2() {
        let mut joinset = JoinSet::new();
        // this test uses two actors sending messages to each other, with the first
        // actor paying (much) higher gas premium than the second.
        // We select with a low ticket quality; the chain depenent merging algorithm
        // should pick messages from the second actor from the start
        let mut mpool = make_test_mpool(&mut joinset);
        let ts = mock_tipset(&mut mpool).await;
        let api = mpool.api.clone();

        // create two actors
        let mut w1 = Wallet::new(KeyStore::new(KeyStoreConfig::Memory).unwrap());
        let a1 = w1.generate_addr(SignatureType::Secp256k1).unwrap();
        let mut w2 = Wallet::new(KeyStore::new(KeyStoreConfig::Memory).unwrap());
        let a2 = w2.generate_addr(SignatureType::Secp256k1).unwrap();

        api.set_state_balance_raw(&a1, TokenAmount::from_whole(1)); // in FIL
        api.set_state_balance_raw(&a2, TokenAmount::from_whole(1)); // in FIL

        let n_msgs = 5 * crate::shim::econ::BLOCK_GAS_LIMIT as i64 / TEST_GAS_LIMIT;
        for i in 0..n_msgs as usize {
            let bias = (n_msgs as usize - i) / 3;
            let m = create_fake_smsg(
                &mpool,
                &a2,
                &a1,
                i as u64,
                TEST_GAS_LIMIT,
                (200000 + i % 3 + bias) as u64,
            );
            mpool.add(m).unwrap();
            let m = create_fake_smsg(
                &mpool,
                &a1,
                &a2,
                i as u64,
                TEST_GAS_LIMIT,
                (190000 + i % 3 + bias) as u64,
            );
            mpool.add(m).unwrap();
        }

        let msgs = mpool.select_messages(&ts, 0.1).unwrap();

        let expected_msgs = crate::shim::econ::BLOCK_GAS_LIMIT as i64 / TEST_GAS_LIMIT;
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
            if m.from() == a1 {
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

        if n_from1 > n_from2 {
            panic!("Expected more msgs from a2 than a1");
        }
    }

    #[tokio::test]
    async fn test_optimal_msg_selection3() {
        let mut joinset = JoinSet::new();
        // this test uses 10 actors sending a block of messages to each other, with the
        // the first actors paying higher gas premium than the subsequent
        // actors. We select with a low ticket quality; the chain depenent
        // merging algorithm should pick messages from the median actor from the
        // start
        let mut mpool = make_test_mpool(&mut joinset);
        let ts = mock_tipset(&mut mpool).await;
        let api = mpool.api.clone();

        let n_actors = 10;

        let mut actors = vec![];
        let mut wallets = vec![];

        for _ in 0..n_actors {
            let mut wallet = Wallet::new(KeyStore::new(KeyStoreConfig::Memory).unwrap());
            let actor = wallet.generate_addr(SignatureType::Secp256k1).unwrap();

            actors.push(actor);
            wallets.push(wallet);
        }

        for a in &mut actors {
            api.set_state_balance_raw(a, TokenAmount::from_whole(1));
        }

        let n_msgs = 1 + crate::shim::econ::BLOCK_GAS_LIMIT as i64 / TEST_GAS_LIMIT;
        for i in 0..n_msgs {
            for j in 0..n_actors {
                let premium =
                    500000 + 10000 * (n_actors - j) + (n_msgs + 2 - i) / (30 * n_actors) + i % 3;
                let m = create_fake_smsg(
                    &mpool,
                    &actors[j as usize],
                    &actors[j as usize],
                    i as u64,
                    TEST_GAS_LIMIT,
                    premium as u64,
                );
                mpool.add(m).unwrap();
            }
        }

        let msgs = mpool.select_messages(&ts, 0.1).unwrap();
        let expected_msgs = crate::shim::econ::BLOCK_GAS_LIMIT as i64 / TEST_GAS_LIMIT;

        assert_eq!(
            msgs.len(),
            expected_msgs as usize,
            "Expected {} messages, but got {}",
            expected_msgs,
            msgs.len()
        );

        let who_is = |addr| -> usize {
            for (i, a) in actors.iter().enumerate() {
                if a == &addr {
                    return i;
                }
            }
            // Lotus has -1, but since we don't have -ve indexes, set it some unrealistic
            // number
            9999999
        };

        let mut nonces = vec![0; n_actors as usize];
        for m in &msgs {
            let who = who_is(m.from());
            if who < 3 {
                panic!("got message from {who}th actor",);
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
