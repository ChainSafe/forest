// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc::eth::filter::{ActorEventBlock, ParsedFilter, ParsedFilterTipsets};
use crate::rpc::eth::{filter::Filter, FilterID};
use crate::rpc::Arc;
use crate::shim::address::Address;
use ahash::AHashMap as HashMap;
use anyhow::{Context, Result};
use parking_lot::RwLock;
use std::any::Any;

#[allow(dead_code)]
#[derive(Debug, PartialEq)]
pub struct EventFilter {
    id: FilterID,
    tipsets: ParsedFilterTipsets,
    addresses: Vec<Address>, // list of actor addresses that are extpected to emit the event
    keys_with_codec: HashMap<String, Vec<ActorEventBlock>>, // map of key names to a list of alternate values that may match
    max_results: usize,                                     // maximum number of results to collect
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
/// by its ID and that a maximum number of results can be configured for each filter.
pub struct EventFilterManager {
    filters: RwLock<HashMap<FilterID, Arc<EventFilter>>>,
    max_filter_results: usize,
}

impl EventFilterManager {
    pub fn new(max_filter_results: usize) -> Arc<Self> {
        Arc::new(Self {
            filters: RwLock::new(HashMap::new()),
            max_filter_results,
        })
    }

    pub fn install(&self, pf: ParsedFilter) -> Result<Arc<EventFilter>> {
        let id = FilterID::new().context("Failed to generate new FilterID")?;

        let filter = Arc::new(EventFilter {
            id: id.clone(),
            tipsets: pf.tipsets,
            addresses: pf.addresses,
            keys_with_codec: pf.keys,
            max_results: self.max_filter_results,
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
    use crate::rpc::eth::filter::{ParsedFilter, ParsedFilterTipsets};
    use crate::shim::address::Address;
    use std::ops::RangeInclusive;

    #[test]
    fn test_event_filter() {
        let max_filter_results = 10;
        let event_manager = EventFilterManager::new(max_filter_results);

        let parsed_filter = ParsedFilter {
            tipsets: ParsedFilterTipsets::Range(RangeInclusive::new(0, 100)),
            addresses: vec![Address::new_id(123)],
            keys: HashMap::new(),
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
            removed,
            Some(filter),
            "Filter should be successfully removed"
        );

        // Verify that the filter is no longer in the event manager
        {
            let filters = event_manager.filters.read();
            assert!(!filters.contains_key(&filter_id));
        }
    }
}
