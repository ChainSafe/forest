// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::utils::{get_gas_perf, get_gas_reward};
use message::{Message, SignedMessage};
use num_bigint::BigInt;
use std::cmp::Ordering;
use std::f64::EPSILON;

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
    pub(crate) fn set_eff_perf(&mut self, prev: Option<(f64, i64)>) {
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
/// Mimics the doubly linked circular-referenced message chain from Lotus by keeping a current index
#[derive(Clone, Debug)]
pub(crate) struct MsgChain {
    index: usize,
    chain: Vec<MsgChainNode>,
}

impl Default for MsgChain {
    fn default() -> Self {
        Self {
            index: 0,
            chain: vec![MsgChainNode::new()],
        }
    }
}

impl MsgChain {
    /// Creates a new message chain
    pub(crate) fn new(nodes: Vec<MsgChainNode>) -> Self {
        Self {
            index: 0,
            chain: nodes,
        }
    }
    /// Retrieves the current node in the MsgChain.
    pub(crate) fn curr(&self) -> &MsgChainNode {
        self.chain.get(self.index).unwrap()
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
    pub(crate) fn curr_mut(&mut self) -> &mut MsgChainNode {
        self.chain.get_mut(self.index).unwrap()
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
    pub(crate) fn compare(&self, other: &Self) -> Ordering {
        let self_curr = self.curr();
        let other_curr = other.curr();
        approx_cmp(self_curr.gas_perf, other_curr.gas_perf)
            .then_with(|| self_curr.gas_reward.cmp(&other_curr.gas_reward))
    }

    pub(crate) fn trim(&mut self, gas_limit: i64, base_fee: &BigInt) {
        let mut i = self.chain.len() as i64 - 1;
        let prev = match self.prev() {
            Some(prev) => Some((prev.eff_perf, prev.gas_limit)),
            None => None,
        };
        let mut mc = self.curr_mut();
        while i >= 0 && (mc.gas_limit > gas_limit || (mc.gas_perf < 0.0)) {
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

        if self.move_forward().is_some() {
            self.invalidate();
            self.move_backward();
        }
    }
    pub(crate) fn invalidate(&mut self) {
        let mc = self.curr_mut();
        mc.valid = false;
        mc.msgs = Vec::new();
        self.chain.drain((self.index + 1)..);
    }
    #[allow(dead_code)]
    pub(crate) fn set_effective_perf(&mut self, bp: f64) {
        self.curr_mut().bp = bp;
        self.set_eff_perf();
    }
    #[allow(dead_code)]
    pub(crate) fn set_eff_perf(&mut self) {
        let prev = match self.prev() {
            Some(prev) => Some((prev.eff_perf, prev.gas_limit)),
            None => None,
        };

        let mc = self.curr_mut();
        let mut eff_perf = mc.gas_perf * mc.bp;
        if let Some(prev) = prev {
            if eff_perf > 0.0 {
                let prev_eff_perf = prev.0;
                let prev_gas_limit = prev.1;
                let eff_perf_with_parent = (eff_perf * mc.gas_limit as f64
                    + prev_eff_perf * prev_gas_limit as f64)
                    / (mc.gas_limit + prev_gas_limit) as f64;
                mc.parent_offset = eff_perf - eff_perf_with_parent;
                eff_perf = eff_perf_with_parent;
            }
        }
        mc.eff_perf = eff_perf;
    }
    #[allow(dead_code)]
    pub fn set_null_effective_perf(&mut self) {
        let mc = self.curr_mut();
        if mc.gas_perf < 0.0 {
            mc.eff_perf = mc.gas_perf;
        } else {
            mc.eff_perf = 0.0;
        }
    }
    #[allow(dead_code)]
    pub(crate) fn cmp_effective(&self, other: &Self) -> Ordering {
        let mc = self.curr();
        let other = other.curr();
        mc.merged
            .cmp(&other.merged)
            .then_with(|| (mc.gas_perf >= 0.0).cmp(&(other.gas_perf >= 0.0)))
            .then_with(|| approx_cmp(mc.eff_perf, other.eff_perf))
            .then_with(|| approx_cmp(mc.gas_perf, other.gas_perf))
            .then_with(|| mc.gas_reward.cmp(&other.gas_reward))
    }
}

fn approx_cmp(a: f64, b: f64) -> Ordering {
    if (a - b).abs() < EPSILON {
        Ordering::Equal
    } else {
        a.partial_cmp(&b).unwrap()
    }
}
