// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use std::{
    cmp::Ordering,
    ops::{Index, IndexMut, RangeFrom},
};

use crate::message::{MessageRead as _, SignedMessage};
use crate::networks::ChainConfig;
use crate::shim::{
    address::Address,
    econ::TokenAmount,
    gas::{Gas, price_list_by_network_version},
};
use crate::{
    blocks::{BLOCK_MESSAGE_LIMIT, Tipset},
    shim::crypto::SignatureType,
};
use ahash::HashMap;
use num_traits::Zero;
use slotmap::{SlotMap, new_key_type};
use tracing::warn;

use super::errors::Error;
use crate::message_pool::{
    provider::Provider,
    utils::{get_gas_perf, get_gas_reward},
};

new_key_type! {
    pub struct NodeKey;
}

/// Chains is an abstraction of a list of message chain nodes.
/// It wraps a `SlotMap` instance. `key_vec` is an additional requirement in
/// order to satisfy optimal `msg` selection use cases, such as iteration in
/// insertion order. The `SlotMap` serves as a lookup table for nodes to get
/// around the borrow checker rules. Each `MsgChainNode` contains only pointers
/// as `NodeKey` to the entries in the map With this design, we get around the
/// borrow checker rule issues when implementing the optimal selection
/// algorithm.
pub(in crate::message_pool) struct Chains {
    pub map: SlotMap<NodeKey, MsgChainNode>,
    pub key_vec: Vec<NodeKey>,
}

impl Chains {
    /// Stable-sorts the chains in `range` best first, where `cmp` returns [`Ordering::Greater`] for
    /// the better chain.
    fn sort_best_first(
        &mut self,
        range: RangeFrom<usize>,
        cmp: impl Fn(&MsgChainNode, &MsgChainNode) -> Ordering,
    ) {
        let map = &self.map;
        let node = |k: &NodeKey| map.get(*k).expect("key_vec keys reference live nodes");
        if let Some(chains) = self.key_vec.get_mut(range) {
            chains.sort_by(|a, b| cmp(node(b), node(a)));
        }
    }

    /// Sorts the chains best first by [`MsgChainNode::compare`].
    pub(in crate::message_pool) fn sort(&mut self) {
        self.sort_best_first(0.., MsgChainNode::compare);
    }

    /// Sorts the chains best first by [`MsgChainNode::cmp_effective`].
    pub(in crate::message_pool) fn sort_effective(&mut self) {
        self.sort_range_effective(0..);
    }

    /// Sorts the chains in `range` best first by [`MsgChainNode::cmp_effective`].
    pub(in crate::message_pool) fn sort_range_effective(&mut self, range: RangeFrom<usize>) {
        self.sort_best_first(range, MsgChainNode::cmp_effective);
    }

    /// Moves the chain at `from` down until it is `before` its successor.
    pub(in crate::message_pool) fn push_down(
        &mut self,
        from: usize,
        before: impl Fn(&MsgChainNode, &MsgChainNode) -> bool,
    ) {
        for i in from..self.key_vec.len().saturating_sub(1) {
            if before(&self[i], &self[i + 1]) {
                break;
            }
            self.key_vec.swap(i, i + 1);
        }
    }

    /// Retrieves the `msg` chain node by the given `NodeKey` along with the
    /// data required from previous chain (if exists) to set effective
    /// performance of this node.
    pub(in crate::message_pool) fn get_mut_with_prev_eff(
        &mut self,
        k: NodeKey,
    ) -> (Option<&mut MsgChainNode>, Option<(f64, u64)>) {
        let node = self.map.get(k);
        let prev = if let Some(node) = node {
            if let Some(prev_key) = node.prev {
                let prev_node = self
                    .map
                    .get(prev_key)
                    .expect("prev key references a live node");
                Some((prev_node.eff_perf, prev_node.gas_limit))
            } else {
                None
            }
        } else {
            None
        };

        let node = self.map.get_mut(k);
        (node, prev)
    }

    /// Retrieves the `msg` chain node by the given `NodeKey`
    pub(in crate::message_pool) fn get(&self, k: NodeKey) -> Option<&MsgChainNode> {
        self.map.get(k)
    }

    /// Sets the effective performance of the chains in each partition from that partition's
    /// block probability, and nullifies it for the chains after them, which fit in no partition.
    /// `partitions` must split a prefix of the sorted chains.
    pub(in crate::message_pool) fn set_effective_perfs(
        &mut self,
        partitions: &[Vec<NodeKey>],
        block_prob: &[f64],
    ) {
        for (partition, &bp) in partitions.iter().zip(block_prob) {
            for &k in partition {
                let (node, prev) = self.get_mut_with_prev_eff(k);
                let node = node.expect("partition keys reference live nodes");
                node.bp = bp;
                node.set_eff_perf(prev);
            }
        }
        let covered = partitions.iter().map(Vec::len).sum();
        for k in self.key_vec.iter().skip(covered) {
            if let Some(node) = self.map.get_mut(*k) {
                node.set_null_effective_perf();
            }
        }
    }
}

impl Chains {
    pub(in crate::message_pool) fn new() -> Self {
        Self {
            map: SlotMap::with_key(),
            key_vec: vec![],
        }
    }

    /// Pushes a `msg` chain node into slot map and places the key in the
    /// `node_vec` passed as parameter.
    pub(in crate::message_pool) fn push_with(
        &mut self,
        cur_chain: MsgChainNode,
        node_vec: &mut Vec<NodeKey>,
    ) {
        let key = self.map.insert(cur_chain);
        node_vec.push(key);
    }

    /// Retrieves the `msg` chain node by the given `NodeKey`
    pub(in crate::message_pool) fn get_mut(&mut self, k: NodeKey) -> Option<&mut MsgChainNode> {
        self.map.get_mut(k)
    }

    /// Retrieves the `msg` chain node at the given index
    pub(in crate::message_pool) fn get_mut_at(&mut self, i: usize) -> Option<&mut MsgChainNode> {
        let key = self.key_vec.get(i)?;
        self.get_mut(*key)
    }

    // Retrieves a msg chain node at the given index in the provided NodeKey vec
    pub(in crate::message_pool) fn get_from(&self, i: usize, vec: &[NodeKey]) -> &MsgChainNode {
        #[allow(clippy::indexing_slicing)]
        self.map
            .get(vec[i])
            .expect("node vec keys reference live nodes")
    }

    // Retrieves a msg chain node at the given index in the provided NodeKey vec
    pub(in crate::message_pool) fn get_mut_from(
        &mut self,
        i: usize,
        vec: &[NodeKey],
    ) -> &mut MsgChainNode {
        #[allow(clippy::indexing_slicing)]
        self.map
            .get_mut(vec[i])
            .expect("node vec keys reference live nodes")
    }

    // Retrieves the node key at the given index
    pub(in crate::message_pool) fn get_key_at(&self, i: usize) -> Option<NodeKey> {
        self.key_vec.get(i).copied()
    }

    /// Retrieves the `msg` chain node at the given index. Returns `None` if index is out-of-bounds.
    pub(in crate::message_pool) fn get_at(&self, i: usize) -> Option<&MsgChainNode> {
        self.map.get(self.get_key_at(i)?)
    }

    /// Retrieves the amount of items.
    pub(in crate::message_pool) fn len(&self) -> usize {
        self.map.len()
    }

    /// Returns true is the chain is empty and otherwise. We check the map as
    /// the source of truth as `key_vec` can be extended time to time.
    pub(in crate::message_pool) fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Removes messages from the given index and resets effective `perfs`
    #[tracing::instrument(skip_all, level = "debug")]
    pub(in crate::message_pool) fn trim_msgs_at(
        &mut self,
        idx: usize,
        gas_limit: u64,
        msg_limit: usize,
        base_fee: &TokenAmount,
    ) {
        let key = self
            .get_key_at(idx)
            .expect("caller validates idx is a live chain");
        let (chain_node, prev) = self.get_mut_with_prev_eff(key);
        let chain_node = chain_node.expect("caller validates idx is a live chain");
        let mut i = chain_node.msgs.len() as i64 - 1;

        while i >= 0
            && (chain_node.gas_limit > gas_limit
                || chain_node.gas_perf < 0.0
                || i >= msg_limit as i64)
        {
            #[allow(clippy::indexing_slicing)]
            let msg = &chain_node.msgs[i as usize];
            let gas_reward = get_gas_reward(msg, base_fee);
            chain_node.gas_reward -= gas_reward;
            chain_node.gas_limit = chain_node.gas_limit.saturating_sub(msg.gas_limit());
            if chain_node.gas_limit > 0 {
                chain_node.gas_perf = get_gas_perf(&chain_node.gas_reward, chain_node.gas_limit);
                if chain_node.bp != 0.0 {
                    chain_node.set_eff_perf(prev);
                }
            } else {
                chain_node.gas_perf = 0.0;
                chain_node.eff_perf = 0.0;
            }
            i -= 1;
        }

        if i < 0 {
            chain_node.msgs.clear();
            chain_node.valid = false;
        } else {
            chain_node.msgs.truncate(i as usize + 1);
        }

        let next = chain_node.next;
        if next.is_some() {
            self.invalidate(next);
        }
    }

    pub(in crate::message_pool) fn invalidate(&mut self, mut key: Option<NodeKey>) {
        let mut next_keys = vec![];

        while let Some(nk) = key {
            let chain_node = self.map.get(nk).expect("chain keys reference live nodes");
            next_keys.push(nk);
            key = chain_node.next;
        }

        for k in next_keys.iter().rev() {
            if let Some(node) = self.map.get_mut(*k) {
                node.valid = false;
                node.msgs.clear();
                node.next = None;
            }
        }
    }

    /// Drops nodes which are no longer valid after the merge step
    pub(in crate::message_pool) fn drop_invalid(&mut self, key_vec: &mut Vec<NodeKey>) {
        let mut valid_keys = vec![];
        for k in key_vec.iter() {
            if self
                .map
                .get(*k)
                .map(|n| n.valid)
                .expect("node vec keys reference live nodes")
            {
                valid_keys.push(*k);
            } else {
                self.map.remove(*k);
            }
        }

        *key_vec = valid_keys;
    }
}

impl Index<usize> for Chains {
    type Output = MsgChainNode;
    fn index(&self, i: usize) -> &Self::Output {
        self.get_at(i).expect("index out of bounds")
    }
}

impl IndexMut<usize> for Chains {
    fn index_mut(&mut self, i: usize) -> &mut Self::Output {
        #[allow(clippy::indexing_slicing)]
        self.map
            .get_mut(self.key_vec[i])
            .expect("key_vec keys reference live nodes")
    }
}

/// Represents a node in the `MsgChain`.
#[derive(Clone, Debug)]
pub struct MsgChainNode {
    pub msgs: Vec<SignedMessage>,
    pub gas_reward: TokenAmount,
    pub gas_limit: u64,
    pub gas_perf: f64,
    pub eff_perf: f64,
    pub bp: f64,
    pub parent_offset: f64,
    pub valid: bool,
    pub merged: bool,
    pub next: Option<NodeKey>,
    pub prev: Option<NodeKey>,
    pub sig_type: Option<SignatureType>,
}

impl MsgChainNode {
    /// Ranks chains by gas performance, then gas reward. [`Ordering::Greater`] means `self` is the better chain.
    pub fn compare(&self, other: &Self) -> Ordering {
        self.gas_perf
            .total_cmp(&other.gas_perf)
            .then_with(|| self.gas_reward.cmp(&other.gas_reward))
    }

    pub fn set_eff_perf(&mut self, prev: Option<(f64, u64)>) {
        let mut eff_perf = self.gas_perf * self.bp;
        if let Some(prev) = prev
            && eff_perf > 0.0
        {
            let prev_eff_perf = prev.0;
            let prev_gas_limit = prev.1;
            let eff_perf_with_parent = (eff_perf * self.gas_limit as f64
                + prev_eff_perf * prev_gas_limit as f64)
                / (self.gas_limit + prev_gas_limit) as f64;
            self.parent_offset = eff_perf - eff_perf_with_parent;
            eff_perf = eff_perf_with_parent;
        }
        self.eff_perf = eff_perf;
    }
}

impl MsgChainNode {
    /// Ranks chains by merged status, non-negative gas performance, effective performance, gas performance, then gas reward.
    /// [`Ordering::Greater`] means `self` is the better chain.
    pub(in crate::message_pool) fn cmp_effective(&self, other: &Self) -> Ordering {
        // Comment from Lotus: move merged chains to the front so we can discard them earlier
        self.merged
            .cmp(&other.merged)
            .then_with(|| (self.gas_perf >= 0.0).cmp(&(other.gas_perf >= 0.0)))
            .then_with(|| self.eff_perf.total_cmp(&other.eff_perf))
            .then_with(|| self.gas_perf.total_cmp(&other.gas_perf))
            .then_with(|| self.gas_reward.cmp(&other.gas_reward))
    }

    /// Push-down counterpart of [`Self::compare`].
    pub(in crate::message_pool) fn before(&self, other: &Self) -> bool {
        self.compare(other) == Ordering::Greater
    }

    /// Push-down counterpart of [`Self::cmp_effective`]. Not a total order (a merged chain and an
    /// unmerged chain with higher effective performance are each "before" the other), so it must
    /// not be used for sorting.
    pub(in crate::message_pool) fn before_effective(&self, other: &Self) -> bool {
        (self.merged && !other.merged)
            || (self.gas_perf >= 0.0 && other.gas_perf < 0.0)
            || self.eff_perf > other.eff_perf
            || (self.eff_perf == other.eff_perf && self.gas_perf > other.gas_perf)
            || (self.eff_perf == other.eff_perf
                && self.gas_perf == other.gas_perf
                && self.gas_reward > other.gas_reward)
    }

    pub fn set_null_effective_perf(&mut self) {
        if self.gas_perf < 0.0 {
            self.eff_perf = self.gas_perf;
        } else {
            self.eff_perf = 0.0;
        }
    }
}

impl std::default::Default for MsgChainNode {
    fn default() -> Self {
        Self {
            msgs: vec![],
            gas_reward: TokenAmount::zero(),
            gas_limit: 0,
            gas_perf: 0.0,
            eff_perf: 0.0,
            bp: 0.0,
            parent_offset: 0.0,
            valid: true,
            merged: false,
            next: None,
            prev: None,
            sig_type: None,
        }
    }
}

pub(in crate::message_pool) fn create_message_chains<T>(
    api: &T,
    actor: &Address,
    mset: &HashMap<u64, SignedMessage>,
    base_fee: &TokenAmount,
    ts: &Tipset,
    chains: &mut Chains,
    chain_config: &ChainConfig,
) -> Result<(), Error>
where
    T: Provider,
{
    // collect all messages and sort
    let mut msgs: Vec<SignedMessage> = mset.values().cloned().collect();
    msgs.sort_by_key(|v| v.sequence());

    // sanity checks:
    // - there can be no gaps in nonces, starting from the current actor nonce if
    //   there is a gap, drop messages after the gap, we can't include them
    // - all messages must have minimum gas and the total gas for the candidate
    //   messages cannot exceed the block limit; drop all messages that exceed the
    //   limit
    // - the total gasReward cannot exceed the actor's balance; drop all messages
    //   that exceed the balance
    let Ok(actor_state) = api.get_actor_after(actor, ts) else {
        tracing::warn!("failed to load actor state, not building chain for {actor}");
        return Ok(());
    };
    let mut cur_seq = actor_state.sequence;
    let mut balance: TokenAmount = TokenAmount::from(&actor_state.balance);

    let mut gas_limit = 0;
    let mut skip = 0;
    let mut i = 0;
    let mut rewards = Vec::with_capacity(msgs.len());

    while let Some(m) = msgs.get(i) {
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

        let network_version = chain_config.network_version(ts.epoch());

        let min_gas = price_list_by_network_version(network_version)
            .on_chain_message(m.chain_length()?)
            .total();

        if Gas::new(m.gas_limit()) < min_gas {
            break;
        }
        gas_limit += m.gas_limit();
        if gas_limit > crate::shim::econ::BLOCK_GAS_LIMIT {
            break;
        }

        let required = m.required_funds();
        if balance < required {
            break;
        }

        balance -= required;
        balance -= m.value();

        let gas_reward = get_gas_reward(m, base_fee);
        rewards.push(gas_reward);
        i += 1;
    }

    // check we have a sane set of messages to construct the chains
    let mut msgs = if i > skip {
        #[allow(clippy::indexing_slicing)]
        msgs[skip..i].to_vec()
    } else {
        return Ok(());
    };

    // if we have more messages from this sender than can fit in a block, drop the extra ones
    if msgs.len() > BLOCK_MESSAGE_LIMIT {
        warn!(
            "dropping {} messages from {actor} as they exceed the block message limit of {BLOCK_MESSAGE_LIMIT}",
            msgs.len() - BLOCK_MESSAGE_LIMIT,
        );
        msgs.truncate(BLOCK_MESSAGE_LIMIT);
    };

    let mut cur_chain = MsgChainNode::default();
    let mut node_vec = vec![];

    let new_chain = |m: SignedMessage, reward: &TokenAmount| -> MsgChainNode {
        let gl = m.gas_limit();
        let sig_type = Some(m.signature().sig_type);
        MsgChainNode {
            msgs: vec![m],
            gas_reward: reward.clone(),
            gas_limit: gl,
            gas_perf: get_gas_perf(reward, gl),
            eff_perf: 0.0,
            bp: 0.0,
            parent_offset: 0.0,
            valid: true,
            merged: false,
            prev: None,
            next: None,
            sig_type,
        }
    };

    // creates msg chain nodes in chunks based on gas_perf obtained from the current
    // chain's gas limit.
    for (i, (m, reward)) in msgs.into_iter().zip(rewards.iter()).enumerate() {
        if i == 0 {
            cur_chain = new_chain(m, reward);
            continue;
        }

        let gas_reward = &cur_chain.gas_reward + reward;
        let gas_limit = cur_chain.gas_limit + m.gas_limit();
        let gas_perf = get_gas_perf(&gas_reward, gas_limit);

        // try to add the message to the current chain -- if it decreases the gasPerf,
        // then make a new chain
        if gas_perf < cur_chain.gas_perf {
            chains.push_with(cur_chain, &mut node_vec);
            cur_chain = new_chain(m, reward);
        } else {
            cur_chain.msgs.push(m);
            cur_chain.gas_reward = gas_reward;
            cur_chain.gas_limit = gas_limit;
            cur_chain.gas_perf = gas_perf;
        }
    }

    chains.push_with(cur_chain, &mut node_vec);

    // merge chains to maintain the invariant: higher gas perf nodes on the front.
    loop {
        let mut merged = 0;
        for i in (1..node_vec.len()).rev() {
            if chains.get_from(i, &node_vec).gas_perf >= chains.get_from(i - 1, &node_vec).gas_perf
            {
                // copy messages
                let chain_i_msg = chains.get_from(i, &node_vec).msgs.clone();
                chains
                    .get_mut_from(i - 1, &node_vec)
                    .msgs
                    .extend(chain_i_msg);

                // set gas reward
                let chain_i_gas_reward = chains.get_from(i, &node_vec).gas_reward.clone();
                chains.get_mut_from(i - 1, &node_vec).gas_reward += chain_i_gas_reward;

                // set gas limit
                let chain_i_gas_limit = chains.get_from(i, &node_vec).gas_limit;
                chains.get_mut_from(i - 1, &node_vec).gas_limit += chain_i_gas_limit;

                // set gas perf
                let chain_i_gas_perf = get_gas_perf(
                    &chains.get_from(i - 1, &node_vec).gas_reward,
                    chains.get_from(i - 1, &node_vec).gas_limit,
                );
                chains.get_mut_from(i - 1, &node_vec).gas_perf = chain_i_gas_perf;
                // invalidate the current chain as it is merged with the prev chain
                chains.get_mut_from(i, &node_vec).valid = false;
                merged += 1;
            }
        }

        if merged == 0 {
            break;
        }

        chains.drop_invalid(&mut node_vec);
    }

    if node_vec.len() > 1 {
        for (&k1, &k2) in node_vec.iter().zip(node_vec.iter().skip(1)) {
            // link next pointers
            let n1 = chains
                .get_mut(k1)
                .ok_or_else(|| Error::Other(format!("{k1:?} should present in `chains`")))?;
            n1.next = Some(k2);
            // Should we link or clear n1.prev as well?

            // link prev pointers
            let n2 = chains
                .get_mut(k2)
                .ok_or_else(|| Error::Other(format!("{k2:?} should present in `chains`")))?;
            n2.prev = Some(k1);
            // Should we link or clear n2.next as well?
        }
    }

    // Update the main chain key_vec with this node_vec
    chains.key_vec.extend(node_vec);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shim::message::Message;
    use itertools::{Itertools as _, iproduct};
    use rand::{Rng as _, SeedableRng as _, rngs::StdRng};
    use rstest::rstest;

    fn sample_nodes() -> Vec<MsgChainNode> {
        let gas_perfs = [
            f64::NEG_INFINITY,
            -1.0,
            -0.0,
            0.0,
            1.0,
            1.0 + f64::EPSILON,
            1e9,
            f64::INFINITY,
            f64::NAN,
        ];
        let eff_perfs = [-1.0, -0.0, 0.0, 1.0, 1.0 + f64::EPSILON, f64::NAN];
        iproduct!(gas_perfs, eff_perfs, [0u64, 1], [false, true])
            .map(|(gas_perf, eff_perf, reward, merged)| node(gas_perf, eff_perf, reward, merged))
            .collect()
    }

    fn node(gas_perf: f64, eff_perf: f64, reward: u64, merged: bool) -> MsgChainNode {
        MsgChainNode {
            gas_perf,
            eff_perf,
            gas_reward: TokenAmount::from_atto(reward),
            merged,
            ..Default::default()
        }
    }

    fn assert_total_order(cmp: impl Fn(&MsgChainNode, &MsgChainNode) -> Ordering) {
        let nodes = sample_nodes();
        for (a, b) in iproduct!(&nodes, &nodes) {
            assert_eq!(
                cmp(a, b),
                cmp(b, a).reverse(),
                "antisymmetry: {a:?} vs {b:?}"
            );
        }
        for (a, b, c) in iproduct!(&nodes, &nodes, &nodes) {
            if cmp(a, b) != Ordering::Greater && cmp(b, c) != Ordering::Greater {
                assert_ne!(
                    cmp(a, c),
                    Ordering::Greater,
                    "transitivity: {a:?}, {b:?}, {c:?}"
                );
            }
        }
    }

    #[rstest]
    #[case::compare(MsgChainNode::compare)]
    #[case::cmp_effective(MsgChainNode::cmp_effective)]
    fn is_total_order(#[case] cmp: fn(&MsgChainNode, &MsgChainNode) -> Ordering) {
        assert_total_order(cmp);
    }

    #[rstest]
    #[case::gas_perf(node(2.0, 0.0, 0, false), node(1.0, 0.0, 9, false), Ordering::Greater)]
    #[case::gas_reward(node(1.0, 0.0, 2, false), node(1.0, 0.0, 1, false), Ordering::Greater)]
    #[case::equal(node(1.0, 0.0, 1, false), node(1.0, 0.0, 1, false), Ordering::Equal)]
    fn compare_key_priority(
        #[case] a: MsgChainNode,
        #[case] b: MsgChainNode,
        #[case] expected: Ordering,
    ) {
        assert_eq!(a.compare(&b), expected);
    }

    #[rstest]
    #[case::merged(node(-1.0, -1.0, 0, true), node(2.0, 2.0, 9, false))]
    #[case::non_negative_gas_perf(node(0.0, -1.0, 0, false), node(-1.0, 0.0, 9, false))]
    #[case::eff_perf(node(1.0, 2.0, 0, false), node(2.0, 1.0, 9, false))]
    #[case::gas_perf(node(2.0, 1.0, 0, false), node(1.0, 1.0, 9, false))]
    #[case::gas_reward(node(1.0, 1.0, 2, false), node(1.0, 1.0, 1, false))]
    fn cmp_effective_key_priority(#[case] better: MsgChainNode, #[case] worse: MsgChainNode) {
        assert_eq!(better.cmp_effective(&worse), Ordering::Greater);
    }

    fn sorted_best_first(
        chains: &Chains,
        cmp: impl Fn(&MsgChainNode, &MsgChainNode) -> Ordering,
    ) -> bool {
        chains
            .key_vec
            .iter()
            .map(|k| &chains.map[*k])
            .tuple_windows()
            .all(|(a, b)| cmp(a, b) != Ordering::Less)
    }

    #[test]
    fn sorting_near_equal_chains_does_not_panic() {
        let mut rng = StdRng::seed_from_u64(0);
        for _ in 0..200 {
            let mut chains = Chains::new();
            let mut keys = vec![];
            for _ in 0..rng.gen_range(2..400) {
                let gas_perf = [-1.0, 0.0, 1.0, 1.0 + f64::EPSILON, 2.0, 3.0][rng.gen_range(0..6)];
                let eff_perf = if rng.gen_bool(0.2) {
                    gas_perf.min(0.0)
                } else {
                    gas_perf * [0.3, 0.5, 0.7, 1.0][rng.gen_range(0..4)]
                };
                let node = node(gas_perf, eff_perf, rng.gen_range(0..3), rng.gen_bool(0.2));
                chains.push_with(node, &mut keys);
            }
            chains.key_vec = keys;
            chains.sort_effective();
            assert!(sorted_best_first(&chains, MsgChainNode::cmp_effective));
            chains.sort();
            assert!(sorted_best_first(&chains, MsgChainNode::compare));
        }
    }

    #[test]
    fn trim_blends_effective_perf_with_own_prev_chain() {
        let base_fee = TokenAmount::from_atto(100u64);
        let smsg = || {
            SignedMessage::mock_bls_signed_message(
                Message::builder()
                    .gas_fee_cap(TokenAmount::from_atto(110u64))
                    .gas_premium(TokenAmount::from_atto(10u64))
                    .gas_limit(100)
                    .build(),
            )
        };
        let mut chains = Chains::new();
        let mut keys = vec![];
        let prev = MsgChainNode {
            eff_perf: 2.0,
            gas_limit: 100,
            ..Default::default()
        };
        chains.push_with(prev, &mut keys);
        // an unrelated chain sorted between the two chains of the same sender
        let unrelated = MsgChainNode {
            eff_perf: 50.0,
            gas_limit: 100,
            ..Default::default()
        };
        chains.push_with(unrelated, &mut keys);
        let msgs = vec![smsg(), smsg()];
        let gas_reward = msgs.iter().fold(TokenAmount::zero(), |acc, m| {
            acc + get_gas_reward(m, &base_fee)
        });
        let node = MsgChainNode {
            gas_perf: get_gas_perf(&gas_reward, 200),
            msgs,
            gas_reward,
            gas_limit: 200,
            bp: 1.0,
            prev: Some(keys[0]),
            ..Default::default()
        };
        chains.push_with(node, &mut keys);
        chains.key_vec = keys;

        chains.trim_msgs_at(2, 100, 10, &base_fee);

        let trimmed = &chains[2];
        assert_eq!(trimmed.msgs.len(), 1);
        assert_eq!(
            trimmed.eff_perf,
            (trimmed.gas_perf * 100.0 + 2.0 * 100.0) / 200.0
        );
    }

    #[test]
    fn set_effective_perfs_blends_child_with_parent() {
        let mut chains = Chains::new();
        let mut keys = vec![];
        let parent = MsgChainNode {
            gas_perf: 10.0,
            gas_limit: 100,
            ..Default::default()
        };
        chains.push_with(parent, &mut keys);
        let child = MsgChainNode {
            gas_perf: 4.0,
            gas_limit: 300,
            prev: Some(keys[0]),
            ..Default::default()
        };
        chains.push_with(child, &mut keys);
        let unpartitioned = MsgChainNode {
            gas_perf: -3.0,
            eff_perf: 7.0,
            ..Default::default()
        };
        chains.push_with(unpartitioned, &mut keys);
        chains.key_vec = keys.clone();

        chains.set_effective_perfs(&[vec![keys[0]], vec![keys[1]]], &[0.5, 0.25]);

        let (parent, child) = (&chains.map[keys[0]], &chains.map[keys[1]]);
        assert_eq!(parent.eff_perf, 5.0);
        assert_eq!(child.bp, 0.25);
        // 4 * 0.25 on its own, (1 * 300 + 5 * 100) / 400 blended with the parent
        assert_eq!(child.eff_perf, 2.0);
        assert_eq!(child.parent_offset, -1.0);
        assert_eq!(chains.map[keys[2]].eff_perf, -3.0);
    }

    #[test]
    fn before_effective_is_lotus_predicate_for_merged_chains() {
        let merged = node(1.0, 1.0, 0, true);
        let better = node(1.0, 2.0, 0, false);
        assert!(merged.before_effective(&better));
        assert!(better.before_effective(&merged));
        assert_eq!(better.cmp_effective(&merged), Ordering::Less);
    }
}
