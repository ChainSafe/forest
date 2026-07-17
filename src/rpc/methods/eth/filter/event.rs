// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::prelude::*;
use crate::rpc::eth::filter::{ActorEventBlock, ParsedFilter, ParsedFilterTipsets};
use crate::rpc::eth::{CollectedEvent, FilterID, SeenEventPositions, filter::Filter};
use crate::shim::address::Address;
use ahash::{HashMap, HashSet};
use anyhow::Result;
use parking_lot::{Mutex, RwLock};
use std::any::Any;

#[derive(Debug)]
pub struct EventFilter {
    // Unique id used to identify the filter
    pub id: FilterID,
    // Tipsets to filter
    pub tipsets: ParsedFilterTipsets,
    // list of actor addresses that are extpected to emit the event
    pub addresses: Vec<Address>,
    // Map of key names to a list of alternate values that may match
    pub keys_with_codec: HashMap<String, Vec<ActorEventBlock>>,
    // Positions of the events returned by the last poll, used to compute the next poll's delta
    seen_positions: Mutex<SeenEventPositions>,
}

impl EventFilter {
    /// Records the positions of `events` as the filter's new poll cursor and
    /// returns only the events the previous poll did not contain.
    pub fn take_unseen(&self, events: Vec<CollectedEvent>) -> Vec<CollectedEvent> {
        let mut seen_positions = self.seen_positions.lock();
        let mut new_positions = SeenEventPositions::default();
        let mut recent_events = Vec::new();
        for event in events {
            let position = (event.msg_idx, event.event_idx);
            let already_seen = seen_positions
                .get(&event.tipset_key)
                .is_some_and(|positions| positions.contains(&position));
            match new_positions.get_mut(&event.tipset_key) {
                Some(positions) => {
                    positions.insert(position);
                }
                None => {
                    new_positions.insert(event.tipset_key.clone(), HashSet::from_iter([position]));
                }
            }
            if !already_seen {
                recent_events.push(event);
            }
        }
        *seen_positions = new_positions;
        recent_events
    }
}

impl From<&EventFilter> for ParsedFilter {
    fn from(event_filter: &EventFilter) -> Self {
        ParsedFilter {
            tipsets: event_filter.tipsets.clone(),
            addresses: event_filter.addresses.clone(),
            keys: event_filter.keys_with_codec.clone(),
            msg_cid: None,
        }
    }
}

impl Filter for EventFilter {
    fn id(&self) -> &FilterID {
        &self.id
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// The `EventFilterManager` structure maintains a set of filters, allowing new filters to be
/// installed or existing ones to be removed. It ensures that each filter is uniquely identifiable
/// by its ID.
pub struct EventFilterManager {
    filters: RwLock<HashMap<FilterID, Arc<EventFilter>>>,
}

impl EventFilterManager {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            filters: RwLock::new(HashMap::new()),
        })
    }

    pub fn install(&self, pf: ParsedFilter) -> Result<Arc<EventFilter>> {
        let id = FilterID::new().context("Failed to generate new FilterID")?;

        let filter = Arc::new(EventFilter {
            id: id.clone(),
            tipsets: pf.tipsets,
            addresses: pf.addresses,
            keys_with_codec: pf.keys,
            seen_positions: Default::default(),
        });

        self.filters.write().insert(id, filter.clone());

        Ok(filter)
    }

    pub fn remove(&self, id: &FilterID) -> Option<Arc<EventFilter>> {
        let mut filters = self.filters.write();
        filters.remove(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blocks::TipsetKey;
    use crate::rpc::eth::filter::{ParsedFilter, ParsedFilterTipsets};
    use crate::shim::address::Address;
    use crate::utils::multihash::MultihashCode;
    use fvm_ipld_encoding::DAG_CBOR;
    use multihash_derive::MultihashDigest as _;
    use nunny::vec as nonempty;
    use std::ops::RangeInclusive;

    fn install_filter() -> Arc<EventFilter> {
        EventFilterManager::new()
            .install(ParsedFilter {
                tipsets: ParsedFilterTipsets::Range(RangeInclusive::new(0, 100)),
                addresses: vec![],
                keys: HashMap::new(),
                msg_cid: None,
            })
            .expect("Failed to install EventFilter")
    }

    fn tipset_key(seed: u8) -> TipsetKey {
        TipsetKey::from(nonempty![Cid::new_v1(
            DAG_CBOR,
            MultihashCode::Identity.digest(&[seed])
        )])
    }

    fn event_at(tipset_key: &TipsetKey, msg_idx: u64, event_idx: u64) -> CollectedEvent {
        CollectedEvent {
            entries: vec![],
            emitter_addr: Address::new_id(0),
            event_idx,
            reverted: false,
            height: 0,
            tipset_key: tipset_key.clone(),
            msg_idx,
            msg_cid: Cid::new_v1(DAG_CBOR, MultihashCode::Identity.digest(&[])),
        }
    }

    #[test]
    fn take_unseen_returns_only_new_events() {
        let filter = install_filter();
        let ts = tipset_key(1);
        let (e0, e1) = (event_at(&ts, 0, 0), event_at(&ts, 0, 1));

        // first poll: everything is new
        assert_eq!(
            filter.take_unseen(vec![e0.clone(), e1.clone()]),
            vec![e0.clone(), e1.clone()]
        );
        // same result set again, nothing new
        assert!(filter.take_unseen(vec![e0.clone(), e1.clone()]).is_empty());
        // a new event alongside the old ones: only the new one comes back
        let e2 = event_at(&ts, 1, 0);
        assert_eq!(filter.take_unseen(vec![e0, e1, e2.clone()]), vec![e2]);
    }

    #[test]
    fn take_unseen_cursor_tracks_only_the_last_poll() {
        let filter = install_filter();
        let (a, b) = (
            event_at(&tipset_key(1), 0, 0),
            event_at(&tipset_key(2), 0, 0),
        );

        assert_eq!(filter.take_unseen(vec![a.clone()]), vec![a.clone()]);
        // a poll without tipset 1 in its results forgets its positions...
        assert_eq!(filter.take_unseen(vec![b.clone()]), vec![b.clone()]);
        // ...so its event counts as new when it reappears, while tipset 2's does not
        assert_eq!(filter.take_unseen(vec![a.clone(), b]), vec![a]);
    }

    #[test]
    fn test_event_filter() {
        let event_manager = EventFilterManager::new();

        let parsed_filter = ParsedFilter {
            tipsets: ParsedFilterTipsets::Range(RangeInclusive::new(0, 100)),
            addresses: vec![Address::new_id(123)],
            keys: HashMap::new(),
            msg_cid: None,
        };
        // Test case 1: Install the EventFilter
        let filter = event_manager
            .install(parsed_filter)
            .expect("Failed to install EventFilter");

        // Verify that the filter has been added to the event manager
        let filter_id = filter.id().clone();
        {
            let filters = event_manager.filters.read();
            assert!(filters.contains_key(&filter_id));
        }

        // Test case 2: Remove the EventFilter
        let removed = event_manager.remove(&filter_id);
        assert_eq!(
            removed.map(|f| f.id().clone()),
            Some(filter_id.clone()),
            "Filter should be successfully removed"
        );

        // Verify that the filter is no longer in the event manager
        {
            let filters = event_manager.filters.read();
            assert!(!filters.contains_key(&filter_id));
        }
    }
}
