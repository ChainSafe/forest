// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::{get_gas_perf, get_gas_reward};
use message::{Message, SignedMessage};
use num_bigint::BigInt;

/// Represents a node in the MsgChain.
#[derive(Clone, Debug)]
pub(crate) struct MsgChainNode {
    pub msgs: Vec<SignedMessage>,
    pub gas_reward: BigInt,
    pub gas_limit: i64,
    pub gas_perf: f64,
    pub eff_perf: f64,
    pub bp: f64,
    pub parent_offset: f64,
    pub valid: bool,
    pub merged: bool,
}

impl MsgChainNode {
    pub(crate) fn new() -> Self {
        Self {
            msgs: vec![],
            gas_reward: Default::default(),
            gas_limit: 0,
            gas_perf: 0.0,
            eff_perf: 0.0,
            bp: 0.0,
            parent_offset: 0.0,
            valid: false,
            merged: false,
        }
    }
    fn set_eff_perf(&mut self, prev: Option<(f64, i64)>) {
        let mut eff_perf = self.gas_perf * self.bp;
        if eff_perf > 0.0 && prev.is_some() {
            let prev = prev.unwrap();
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
/// Mimics the doubly linked circular-referenced message chain from Lotus by keeping a current index
#[derive(Clone, Debug)]
pub(crate) struct MsgChain {
    pub index: usize,
    pub chain: Vec<MsgChainNode>,
}

impl MsgChain {
    /// Creates a new message chain
    pub(crate) fn new() -> Self {
        Self {
            index: 0,
            chain: vec![MsgChainNode::new()],
        }
    }
    /// Retrieves the current node in the MsgChain.
    /// This should never be None if created through the constructor.
    pub(crate) fn curr(&self) -> Option<&MsgChainNode> {
        self.chain.get(self.index)
    }
    /// Retrieves the previous element in the MsgChain.
    pub(crate) fn prev(&self) -> Option<&MsgChainNode> {
        if self.index == 0 {
            return None;
        }
        self.chain.get(self.index - 1)
    }
    /// Retrieves the next element in the MsgChain.
    #[allow(dead_code)]
    pub(crate) fn next(&self) -> Option<&MsgChainNode> {
        if self.index == self.chain.len() - 1 {
            return None;
        }
        self.chain.get(self.index + 1)
    }
    /// Retrieves a mutable reference to the current node in the MsgChain.
    /// This should never be None if created through the constructor.
    pub(crate) fn curr_mut(&mut self) -> Option<&mut MsgChainNode> {
        self.chain.get_mut(self.index)
    }
    /// Retrieves a mutable reference to the previous element in the MsgChain.
    #[allow(dead_code)]
    pub(crate) fn prev_mut(&mut self) -> Option<&mut MsgChainNode> {
        if self.index == 0 {
            return None;
        }
        self.chain.get_mut(self.index - 1)
    }
    /// Retrieves a mutable reference to the next element in the MsgChain.
    #[allow(dead_code)]
    pub(crate) fn next_mut(&mut self) -> Option<&mut MsgChainNode> {
        if self.index == self.chain.len() - 1 {
            return None;
        }
        self.chain.get_mut(self.index + 1)
    }
    /// Advances the current index forward and returns the new current node.
    pub(crate) fn move_forward(&mut self) -> Option<&MsgChainNode> {
        if self.index == self.chain.len() - 1 {
            return None;
        }
        self.index += 1;
        self.chain.get(self.index)
    }
    /// Advances the current index backward and returns the new current node.
    pub(crate) fn move_backward(&mut self) -> Option<&MsgChainNode> {
        if self.index == 0 {
            return None;
        }
        self.index -= 1;
        self.chain.get(self.index)
    }
}

impl MsgChain {
    pub(crate) fn before(&self, other: &MsgChain) -> bool {
        let self_curr = self.curr().unwrap();
        let other_curr = other.curr().unwrap();
        self_curr.gas_perf > other_curr.gas_perf
            || (self_curr.gas_perf == other_curr.gas_perf
                && self_curr.gas_reward < other_curr.gas_reward)
    }

    pub(crate) fn trim(&mut self, gas_limit: i64, base_fee: &BigInt, allow_negative: bool) {
        let mut i = self.chain.len() as i64 - 1;
        let prev = match self.prev() {
            Some(prev) => Some((prev.eff_perf, prev.gas_limit)),
            None => None,
        };
        let mut mc = self.curr_mut().unwrap();
        while i >= 0 && (mc.gas_limit > gas_limit || (!allow_negative && mc.gas_perf < 0.0)) {
            let gas_reward = get_gas_reward(&mc.msgs[i as usize], base_fee);
            mc.gas_reward -= gas_reward;
            mc.gas_limit -= mc.msgs[i as usize].gas_limit();
            if mc.gas_limit > 0 {
                mc.gas_perf = get_gas_perf(&mc.gas_reward, mc.gas_limit);
                if mc.bp != 0.0 {
                    // set eff perf
                    mc.set_eff_perf(prev);
                }
            } else {
                mc.gas_perf = 0.0;
                mc.eff_perf = 0.0;
            }
            i -= 1;
        }
        if i < 0 {
            mc.msgs = Vec::new();
            mc.valid = false;
        } else {
            mc.msgs.drain(0..i as usize);
        }

        if let Some(_) = self.move_forward() {
            self.invalidate();
            self.move_backward();
            self.chain.remove(self.chain.len() - 1);
        }
    }
    pub(crate) fn invalidate(&mut self) {
        let mc = self.curr_mut().unwrap();
        mc.valid = false;
        mc.msgs = Vec::new();
        self.chain.drain(self.index..);
    }
    #[allow(dead_code)]
    pub(crate) fn set_effective_perf(&mut self, bp: f64) {
        self.curr_mut().unwrap().bp = bp;
        self.set_eff_perf();
    }
    #[allow(dead_code)]
    fn set_eff_perf(&mut self) {
        let prev = match self.prev() {
            Some(prev) => Some((prev.eff_perf, prev.gas_limit)),
            None => None,
        };

        let mc = self.curr_mut().unwrap();
        let mut eff_perf = mc.gas_perf * mc.bp;
        if eff_perf > 0.0 && prev.is_some() {
            let prev = prev.unwrap();
            let prev_eff_perf = prev.0;
            let prev_gas_limit = prev.1;
            let eff_perf_with_parent = (eff_perf * mc.gas_limit as f64
                + prev_eff_perf * prev_gas_limit as f64)
                / (mc.gas_limit + prev_gas_limit) as f64;
            mc.parent_offset = eff_perf - eff_perf_with_parent;
            eff_perf = eff_perf_with_parent;
        }
        mc.eff_perf = eff_perf;
    }
    #[allow(dead_code)]
    pub fn set_null_effective_perf(&mut self) {
        let mc = self.curr_mut().unwrap();
        if mc.gas_perf < 0.0 {
            mc.eff_perf = mc.gas_perf;
        } else {
            mc.eff_perf = 0.0;
        }
    }
    #[allow(dead_code)]
    pub fn before_effective(&self, other: &MsgChain) -> bool {
        let mc = self.curr().unwrap();
        let other = other.curr().unwrap();
        (mc.merged && !other.merged)
            || (mc.gas_perf >= 0.0 && other.gas_perf < 0.0)
            || (mc.eff_perf > other.eff_perf)
            || (mc.eff_perf == other.eff_perf && mc.gas_perf > other.gas_perf)
            || (mc.eff_perf == other.eff_perf
                && mc.gas_perf == other.gas_perf
                && mc.gas_reward > other.gas_reward)
    }
}
