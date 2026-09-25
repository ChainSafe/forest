// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! The events of one executed tipset, collected once and served to every question about them.
//!
//! [`TipsetEvents`] is the chain-level artifact: every event of the tipset in message order,
//! numbered, with its emitter resolved to a deterministic address where the tipset's state
//! allows. [`BlockLogs`] is its Ethereum projection. Construction is the only step that performs
//! I/O; every view over the artifact is pure.

mod logs;

pub use logs::BlockLogs;
pub(super) use logs::eth_log_from_event;

use crate::blocks::{Tipset, TipsetKey};
use crate::chain::AtFinalityResolution;
use crate::db::DbImpl;
use crate::rpc::eth::types::EthHash;
use crate::rpc::types::EventEntry;
use crate::shim::address::Address;
use crate::shim::clock::ChainEpoch;
use crate::shim::executor::Entry;
use crate::shim::fvm_shared_latest::ActorID;
use crate::shim::state_tree::StateTree;
use crate::state_manager::{ExecutedTipset, StateManager};
use ahash::HashMap;
use cid::Cid;
use std::cell::OnceCell;

/// Every event of one executed tipset, numbered once and emitter-resolved once.
pub struct TipsetEvents {
    tipset_key: TipsetKey,
    height: ChainEpoch,
    message_count: usize,
    /// The messages that emitted events, in message order.
    messages: Vec<MessageEvents>,
}

/// The events of one message, with the identity all of them share.
struct MessageEvents {
    msg_idx: u64,
    eth_tx_hash: Option<EthHash>,
    events: Vec<TipsetEvent>,
}

struct TipsetEvent {
    /// Position among all events of the tipset, counting every event of every message.
    event_idx: u64,
    /// The emitter's deterministic address, when the tipset's state resolves one.
    emitter_address: Option<Address>,
    entries: Vec<EventEntry>,
}

impl TipsetEvents {
    /// Collects the events of an executed tipset.
    pub fn collect(
        state_manager: &StateManager,
        tipset: &Tipset,
        executed: &ExecutedTipset,
    ) -> Self {
        let eth_chain_id = state_manager.chain_config().eth_chain_id;
        let mut resolver = EmitterResolver::new(state_manager, tipset, &executed.state_root);
        let mut event_idx = 0u64;
        let mut messages = Vec::new();
        for (msg_idx, executed_message) in executed.executed_messages.iter().enumerate() {
            let Some(events) = executed_message
                .events
                .as_deref()
                .filter(|events| !events.is_empty())
            else {
                continue;
            };
            let events = events
                .iter()
                .map(|event| {
                    let collected = TipsetEvent {
                        event_idx,
                        emitter_address: resolver.resolve(event.emitter()),
                        entries: event.entries().into_iter().map(event_entry).collect(),
                    };
                    event_idx += 1;
                    collected
                })
                .collect();
            messages.push(MessageEvents {
                msg_idx: msg_idx as u64,
                eth_tx_hash: logs::eth_tx_hash(&executed_message.message, eth_chain_id),
                events,
            });
        }
        Self {
            tipset_key: tipset.key().clone(),
            height: tipset.epoch(),
            message_count: executed.executed_messages.len(),
            messages,
        }
    }
}

fn event_entry(entry: Entry) -> EventEntry {
    let (flags, key, codec, value) = entry.into_parts();
    EventEntry {
        flags,
        key,
        codec,
        value: value.into(),
    }
}

/// Resolves event emitters to deterministic addresses without awaiting: construction runs inside
/// the executed-tipset cache fill, where awaiting a state cache deadlocks.
struct EmitterResolver<'a> {
    state_manager: &'a StateManager,
    tipset: &'a Tipset,
    state_root: &'a Cid,
    post_state: OnceCell<Option<StateTree<DbImpl>>>,
    resolved: HashMap<ActorID, Option<Address>>,
}

impl<'a> EmitterResolver<'a> {
    fn new(state_manager: &'a StateManager, tipset: &'a Tipset, state_root: &'a Cid) -> Self {
        Self {
            state_manager,
            tipset,
            state_root,
            post_state: OnceCell::new(),
            resolved: HashMap::default(),
        }
    }

    fn resolve(&mut self, emitter: ActorID) -> Option<Address> {
        if let Some(resolved) = self.resolved.get(&emitter) {
            return *resolved;
        }
        let resolved = self.resolve_uncached(emitter);
        self.resolved.insert(emitter, resolved);
        resolved
    }

    /// Global cache first, then the finality-deep state (cached globally when reorg-stable),
    /// then the post-execution state, the only one holding an actor created in this tipset.
    fn resolve_uncached(&self, emitter: ActorID) -> Option<Address> {
        let cache = self.state_manager.id_to_deterministic_address_cache();
        if let Some(address) = cache.and_then(|cache| cache.get(&emitter)) {
            return Some(address);
        }
        let id_address = Address::new_id(emitter);
        match self
            .state_manager
            .chain_store()
            .resolve_to_deterministic_address_at_finality(&id_address, self.tipset)
        {
            Ok(AtFinalityResolution::ReorgStable(address)) => {
                if let Some(cache) = cache {
                    cache.insert(emitter, address);
                }
                return Some(address);
            }
            Ok(AtFinalityResolution::Unstable(address)) => return Some(address),
            Err(_) => {}
        }
        let db = self.state_manager.db();
        self.post_state
            .get_or_init(|| self.state_manager.get_state_tree(self.state_root).ok())
            .as_ref()?
            .resolve_to_deterministic_address(db, id_address)
            .ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::ChainMessage;
    use crate::shim::executor::{Receipt, StampedEvent};
    use crate::shim::message::Message;
    use crate::state_manager::ExecutedMessage;
    use std::sync::Arc;

    /// Minimal `StateManager` over an in-memory store plus its (genesis) heaviest tipset.
    pub(super) fn test_state_manager() -> (StateManager, Tipset) {
        use crate::blocks::{CachingBlockHeader, RawBlockHeader};
        use crate::chain::ChainStore;
        use crate::db::MemoryDB;
        use crate::networks::ChainConfig;

        let db = Arc::new(MemoryDB::default());
        let genesis_header = CachingBlockHeader::new(RawBlockHeader {
            miner_address: Address::new_id(0),
            // A zero genesis timestamp is rejected by the beacon schedule.
            timestamp: 7777,
            ..Default::default()
        });
        let chain_store =
            ChainStore::new(db, Arc::new(ChainConfig::default()), genesis_header).unwrap();
        let tipset = chain_store.heaviest_tipset();
        let state_manager = StateManager::new(chain_store).unwrap();
        (state_manager, tipset)
    }

    /// `transaction_index` and `log_index` derive from these positions: the message's index in
    /// `executed_messages` and a gap-free count across the whole tipset (NUM-1, NUM-5).
    #[test]
    fn collect_numbers_events_by_message_and_across_the_tipset() {
        let (state_manager, tipset) = test_state_manager();

        let exec_msg = |n_events: usize| ExecutedMessage {
            message: ChainMessage::Unsigned(Message::default().into()),
            receipt: Receipt::empty_success(),
            events: (n_events > 0).then(|| {
                (0..n_events)
                    .map(|_| StampedEvent::new_indexed(1000, "t1"))
                    .collect()
            }),
        };
        // Message 1 emits nothing and must not shift the indices of later messages' events.
        let events_per_msg = [2usize, 0, 3, 1];
        let executed = ExecutedTipset {
            state_root: Cid::default(),
            receipt_root: Cid::default(),
            executed_messages: Arc::new(events_per_msg.iter().map(|&n| exec_msg(n)).collect()),
        };

        let events = TipsetEvents::collect(&state_manager, &tipset, &executed);

        let expected = events_per_msg
            .iter()
            .enumerate()
            .flat_map(|(msg_idx, &n)| std::iter::repeat_n(msg_idx as u64, n))
            .enumerate()
            .map(|(event_idx, msg_idx)| (msg_idx, event_idx as u64));
        itertools::assert_equal(
            events.messages.iter().flat_map(|message| {
                message
                    .events
                    .iter()
                    .map(|e| (message.msg_idx, e.event_idx))
            }),
            expected,
        );
        assert_eq!(events.message_count, events_per_msg.len());
        // The test genesis has no state tree, so no emitter resolves.
        assert!(
            events
                .messages
                .iter()
                .flat_map(|message| &message.events)
                .all(|event| event.emitter_address.is_none())
        );
    }
}
