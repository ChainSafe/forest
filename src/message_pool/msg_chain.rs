// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{
    cmp::Ordering,
    mem,
    ops::{Index, IndexMut},
};

use crate::blocks::Tipset;
use crate::message::{Message, SignedMessage};
use crate::networks::ChainConfig;
use crate::shim::{
    address::Address,
    econ::TokenAmount,
    gas::{price_list_by_network_version, Gas},
};
use ahash::HashMap;
use fvm_ipld_encoding::Cbor;
use log::warn;
use num_traits::Zero;
use slotmap::{new_key_type, SlotMap};

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

#[cfg(test)]
impl Chains {
    // Sort by effective perf with cmp_effective
    pub(in crate::message_pool) fn sort_effective(&mut self) {
        let mut chains = mem::take(&mut self.key_vec);
        chains.sort_by(|a, b| {
            let a = self.map.get(*a).unwrap();
            let b = self.map.get(*b).unwrap();
            a.cmp_effective(b)
        });
        let _ = mem::replace(&mut self.key_vec, chains);
    }

    // Sort by effective `perf` on a range
    pub(in crate::message_pool) fn sort_range_effective(
        &mut self,
        range: std::ops::RangeFrom<usize>,
    ) {
        let mut chains = mem::take(&mut self.key_vec);
        chains[range].sort_by(|a, b| {
            self.map
                .get(*a)
                .unwrap()
                .cmp_effective(self.map.get(*b).unwrap())
        });
        let _ = mem::replace(&mut self.key_vec, chains);
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
                let prev_node = self.map.get(prev_key).unwrap();
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

    /// Sorts the chains with `compare` method. If rev is true, sorts in
    /// descending order.
    pub(in crate::message_pool) fn sort(&mut self, rev: bool) {
        // replace dance to get around borrow checker
        let mut chains = mem::take(&mut self.key_vec);
        chains.sort_by(|a, b| {
            let a = self.map.get(*a).unwrap();
            let b = self.map.get(*b).unwrap();
            if rev {
                b.compare(a)
            } else {
                a.compare(b)
            }
        });
        let _ = mem::replace(&mut self.key_vec, chains);
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
        self.map.get(vec[i]).unwrap()
    }

    // Retrieves a msg chain node at the given index in the provided NodeKey vec
    pub(in crate::message_pool) fn get_mut_from(
        &mut self,
        i: usize,
        vec: &[NodeKey],
    ) -> &mut MsgChainNode {
        self.map.get_mut(vec[i]).unwrap()
    }

    // Retrieves the node key at the given index
    pub(in crate::message_pool) fn get_key_at(&self, i: usize) -> Option<NodeKey> {
        self.key_vec.get(i).copied()
    }

    /// Retrieves the `msg` chain node at the given index
    pub(in crate::message_pool) fn get_at(&mut self, i: usize) -> Option<&MsgChainNode> {
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
    pub(in crate::message_pool) fn trim_msgs_at(
        &mut self,
        idx: usize,
        gas_limit: u64,
        base_fee: &TokenAmount,
    ) {
        let prev = match idx {
            0 => None,
            _ => self
                .get_at(idx - 1)
                .map(|prev| (prev.eff_perf, prev.gas_limit)),
        };
        let chain_node = self.get_mut_at(idx).unwrap();
        let mut i = chain_node.msgs.len() as i64 - 1;

        while i >= 0 && (chain_node.gas_limit > gas_limit || (chain_node.gas_perf < 0.0)) {
            let gas_reward = get_gas_reward(&chain_node.msgs[i as usize], base_fee);
            chain_node.gas_reward -= gas_reward;
            chain_node.gas_limit -= chain_node.msgs[i as usize].gas_limit();
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
            chain_node.msgs.drain(0..i as usize + 1);
        }

        let next = chain_node.next;
        if next.is_some() {
            self.invalidate(next);
        }
    }

    pub(in crate::message_pool) fn invalidate(&mut self, mut key: Option<NodeKey>) {
        let mut next_keys = vec![];

        while let Some(nk) = key {
            let chain_node = self.map.get(nk).unwrap();
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
            if let true = self.map.get(*k).map(|n| n.valid).unwrap() {
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
        self.map.get(self.key_vec[i]).unwrap()
    }
}

impl IndexMut<usize> for Chains {
    fn index_mut(&mut self, i: usize) -> &mut Self::Output {
        self.map.get_mut(self.key_vec[i]).unwrap()
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
}

impl MsgChainNode {
    pub fn compare(&self, other: &Self) -> Ordering {
        if approx_cmp(self.gas_perf, other.gas_perf) == Ordering::Greater
            || approx_cmp(self.gas_perf, other.gas_perf) == Ordering::Equal
                && self.gas_reward.cmp(&other.gas_reward) == Ordering::Greater
        {
            return Ordering::Greater;
        }

        Ordering::Less
    }

    pub fn set_eff_perf(&mut self, prev: Option<(f64, u64)>) {
        let mut eff_perf = self.gas_perf * self.bp;
        if let Some(prev) = prev {
            if eff_perf > 0.0 {
                let prev_eff_perf = prev.0;
                let prev_gas_limit = prev.1;
                let eff_perf_with_parent = (eff_perf * self.gas_limit as f64
                    + prev_eff_perf * prev_gas_limit as f64)
                    / (self.gas_limit + prev_gas_limit) as f64;
                self.parent_offset = eff_perf - eff_perf_with_parent;
                eff_perf = eff_perf_with_parent;
            }
        }
        self.eff_perf = eff_perf;
    }
}

#[cfg(test)]
impl MsgChainNode {
    pub(in crate::message_pool) fn cmp_effective(&self, other: &Self) -> Ordering {
        if self.merged && !other.merged
            || self.gas_perf >= 0.0 && other.gas_perf < 0.0
            || self.eff_perf > other.eff_perf
            || (approx_cmp(self.eff_perf, other.eff_perf) == Ordering::Equal
                && self.gas_perf > other.gas_perf)
            || (approx_cmp(self.eff_perf, other.eff_perf) == Ordering::Equal
                && approx_cmp(self.gas_perf, other.gas_perf) == Ordering::Equal
                && self.gas_reward > other.gas_reward)
        {
            return Ordering::Greater;
        }

        Ordering::Less
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
    let actor_state = api.get_actor_after(actor, ts)?;
    let mut cur_seq = actor_state.sequence;
    let mut balance: TokenAmount = TokenAmount::from(&actor_state.balance);

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

        let network_version = chain_config.network_version(ts.epoch());

        let min_gas = price_list_by_network_version(network_version)
            .on_chain_message(m.marshal_cbor()?.len())
            .total();

        if Gas::new(m.gas_limit()) < min_gas {
            break;
        }
        gas_limit += m.gas_limit();
        if gas_limit > fvm_shared3::BLOCK_GAS_LIMIT {
            break;
        }

        let required = m.required_funds();
        if balance < required {
            break;
        }

        balance -= required;
        let value = m.value();
        balance -= value;

        let gas_reward = get_gas_reward(m, base_fee);
        rewards.push(gas_reward);
        i += 1;
    }

    // check we have a sane set of messages to construct the chains
    let msgs = if i > skip {
        msgs[skip..i].to_vec()
    } else {
        return Ok(());
    };

    let mut cur_chain = MsgChainNode::default();
    let mut node_vec = vec![];

    let new_chain = |m: SignedMessage, i: usize| -> MsgChainNode {
        let gl = m.gas_limit();
        MsgChainNode {
            msgs: vec![m],
            gas_reward: rewards[i].clone(),
            gas_limit: gl,
            gas_perf: get_gas_perf(&rewards[i], gl),
            eff_perf: 0.0,
            bp: 0.0,
            parent_offset: 0.0,
            valid: true,
            merged: false,
            prev: None,
            next: None,
        }
    };

    // creates msg chain nodes in chunks based on gas_perf obtained from the current
    // chain's gas limit.
    for (i, m) in msgs.into_iter().enumerate() {
        if i == 0 {
            cur_chain = new_chain(m, i);
            continue;
        }

        let gas_reward = cur_chain.gas_reward.clone() + &rewards[i];
        let gas_limit = cur_chain.gas_limit + m.gas_limit();
        let gas_perf = get_gas_perf(&gas_reward, gas_limit);

        // try to add the message to the current chain -- if it decreases the gasPerf,
        // then make a new chain
        if gas_perf < cur_chain.gas_perf {
            chains.push_with(cur_chain, &mut node_vec);
            cur_chain = new_chain(m, i);
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

    // link next pointers
    for i in 0..node_vec.len() - 1 {
        let k1 = node_vec.get(i).unwrap();
        let k2 = node_vec.get(i + 1);
        let n1 = chains.get_mut(*k1).unwrap();
        n1.next = k2.cloned();
    }

    // link prev pointers
    for i in (0..node_vec.len() - 1).rev() {
        let k1 = node_vec.get(i);
        let k2 = node_vec.get(i + 1).unwrap();
        let n2 = chains.get_mut(*k2).unwrap();
        n2.prev = k1.cloned();
    }

    // Update the main chain key_vec with this node_vec
    chains.key_vec.extend(node_vec.iter());

    Ok(())
}

fn approx_cmp(a: f64, b: f64) -> Ordering {
    if (a - b).abs() <= (a * std::f64::EPSILON).abs() {
        Ordering::Equal
    } else {
        a.partial_cmp(&b).unwrap()
    }
}
