// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Tracks which CIDs were already broadcast in the current republish cycle
//! and exposes a trigger to wake the republish task early.

use std::cmp::Ordering;

use ahash::{HashMap, HashMapExt, HashSet};
use cid::Cid;
use parking_lot::RwLock as SyncRwLock;

use crate::message::{MessageRead as _, SignedMessage};
use crate::message_pool::{
    Error,
    msg_chain::{Chains, create_message_chains},
    msgpool::{MIN_GAS, msg_pool::MessagePool},
    provider::Provider,
    utils::get_base_fee_lower_bound,
};
use crate::prelude::ShallowClone;
use crate::shim::address::Address;
use crate::utils::ShallowClone as _;

const REPUB_TRIGGER_CAPACITY: usize = 1;
const BASE_FEE_LOWER_BOUND_FACTOR: i64 = 10;
const REPUB_MSG_LIMIT: usize = 30;

pub(in crate::message_pool) struct RepublishState {
    republished: SyncRwLock<HashSet<Cid>>,
    trigger: flume::Sender<()>,
}

impl RepublishState {
    pub(in crate::message_pool) fn new() -> (Self, flume::Receiver<()>) {
        let (trigger, rx) = flume::bounded(REPUB_TRIGGER_CAPACITY);
        (
            Self {
                republished: SyncRwLock::default(),
                trigger,
            },
            rx,
        )
    }

    /// Returns `true` if `cid` was seen by the republished state.
    pub(in crate::message_pool) fn was_republished(&self, cid: &Cid) -> bool {
        self.republished.read().contains(cid)
    }

    /// Wake the republish task early.
    pub(in crate::message_pool) fn trigger(&self) -> Result<(), Error> {
        match self.trigger.try_send(()) {
            Ok(()) | Err(flume::TrySendError::Full(_)) => Ok(()),
            Err(flume::TrySendError::Disconnected(_)) => {
                Err(Error::Other("republish receiver dropped".into()))
            }
        }
    }

    pub(in crate::message_pool) fn replace_with<I: IntoIterator<Item = Cid>>(&self, cids: I) {
        let mut set = self.republished.write();
        set.clear();
        set.extend(cids);
    }
}

impl<T: Provider> MessagePool<T> {
    pub(in crate::message_pool) async fn run_republish_cycle(&self) -> Result<(), Error> {
        let ts = self.cur_tipset.read().shallow_clone();

        // Only republish messages from local addresses, i.e., transactions which
        // were sent to this node directly.
        let local: Vec<Address> = self.local_addrs.read().iter().copied().collect();
        let mut pending_map: HashMap<Address, HashMap<u64, SignedMessage>> =
            HashMap::with_capacity(local.len());
        for actor in &local {
            if let Some(mset) = self.pending.snapshot_for(actor)
                && !mset.msgs.is_empty()
            {
                pending_map.insert(*actor, mset.msgs);
            }
        }

        let msgs =
            select_messages_to_republish(self.api.as_ref(), &self.chain_config, &ts, pending_map)?;

        for m in msgs.iter() {
            self.publish_pubsub(m).await?;
        }

        self.republish.replace_with(msgs.iter().map(|m| m.cid()));

        Ok(())
    }
}

/// Score local senders' pending message chains for the republish broadcast.
///
/// Distinct from the block-producer selection path (`selection.rs`): uses
/// the aggressive [`BASE_FEE_LOWER_BOUND_FACTOR`] of 10 (vs. 100 in the add
/// path) and caps the result at [`REPUB_MSG_LIMIT`] messages.
fn select_messages_to_republish<T>(
    api: &T,
    chain_config: &crate::networks::ChainConfig,
    base: &crate::blocks::Tipset,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn was_republished_reflects_replace_with() {
        let (state, _rx) = RepublishState::new();
        let cid = Cid::default();

        assert!(
            !state.was_republished(&cid),
            "fresh state should not contain any CIDs",
        );

        state.replace_with([cid]);
        assert!(
            state.was_republished(&cid),
            "replace_with should populate the set",
        );

        state.replace_with(std::iter::empty());
        assert!(
            !state.was_republished(&cid),
            "replace_with with empty iter should clear the set",
        );
    }

    #[test]
    fn trigger_succeeds_when_receiver_is_alive() {
        let (state, rx) = RepublishState::new();
        state.trigger().expect("send should succeed");
        rx.try_recv()
            .expect("trigger should be observable on the receiver");
    }

    #[test]
    fn trigger_drops_silently_when_buffer_full() {
        let (state, _rx) = RepublishState::new();
        state.trigger().expect("first trigger should send");
        // Buffer (capacity 1) is now full; a second trigger must coalesce
        // silently instead of failing head_change.
        state
            .trigger()
            .expect("overflow trigger should be dropped silently");
    }

    #[test]
    fn trigger_errors_when_receiver_disconnected() {
        let (state, rx) = RepublishState::new();
        drop(rx);
        let err = state
            .trigger()
            .expect_err("disconnected receiver should surface as an error");
        assert!(matches!(err, Error::Other(_)));
    }
}
