// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc::eth::filter::ActorEventBlock;
use crate::rpc::eth::filter::ParsedFilter;
use crate::rpc::eth::CollectedEvent;
use crate::rpc::eth::{filter::Filter, FilterID};
use crate::rpc::Arc;
use crate::shim::address::Address;
use crate::shim::clock::ChainEpoch;
use ahash::AHashMap as HashMap;
use anyhow::{Context, Result};
use cid::Cid;
use parking_lot::RwLock;

#[allow(dead_code)]
#[derive(Debug)]
pub struct EventFilter {
    id: FilterID,
    min_height: ChainEpoch, // minimum epoch to apply filter
    max_height: ChainEpoch, // maximum epoch to apply filter
    tipset_cid: Cid,
    addresses: Vec<Address>, // list of actor addresses that are extpected to emit the event
    keys_with_codec: HashMap<String, Vec<ActorEventBlock>>, // map of key names to a list of alternate values that may match
    max_results: usize,                                     // maximum number of results to collect
}

impl Filter for EventFilter {
    fn id(&self) -> &FilterID {
        &self.id
    }
    fn take_collected_events(&self) -> Vec<CollectedEvent> {
        vec![]
    }
}

pub struct EventIndex {}

impl EventIndex {
    pub fn is_height_past(&self, _height: ChainEpoch) -> anyhow::Result<bool> {
        todo!()
    }
}

/// The `EventFilterManager` structure maintains a set of filters, allowing new filters to be
/// installed or existing ones to be removed. It ensures that each filter is uniquely identifiable
/// by its ID and that a maximum number of results can be configured for each filter.
pub struct EventFilterManager {
    filters: RwLock<HashMap<FilterID, Arc<EventFilter>>>,
    max_filter_results: usize,

    // TODO(elmattic): implement similar functionality
    pub event_index: Option<Arc<EventIndex>>,
}

impl EventFilterManager {
    pub fn new(max_filter_results: usize) -> Arc<Self> {
        Arc::new(Self {
            filters: RwLock::new(HashMap::new()),
            max_filter_results,
            event_index: None,
        })
    }

    pub fn install(&self, pf: ParsedFilter) -> Result<Arc<dyn Filter>> {
        let id = FilterID::new().context("Failed to generate new FilterID")?;

        let filter = Arc::new(EventFilter {
            id: id.clone(),
            min_height: pf.min_height,
            max_height: pf.max_height,
            tipset_cid: pf.tipset_cid,
            addresses: pf.addresses,
            keys_with_codec: pf.keys,
            max_results: self.max_filter_results,
        });

        self.filters.write().insert(id, filter.clone());

        Ok(filter)
    }

    pub fn remove(&self, id: &FilterID) -> bool {
        let mut filters = self.filters.write();
        filters.remove(id).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rpc::eth::filter::ParsedFilter;
    use crate::shim::address::Address;
    use crate::shim::clock::ChainEpoch;
    use cid::Cid;

    #[test]
    fn test_event_filter() {
        let max_filter_results = 10;
        let event_manager = EventFilterManager::new(max_filter_results);

        let parsed_filter = ParsedFilter {
            min_height: ChainEpoch::from(0),
            max_height: ChainEpoch::from(100),
            tipset_cid: Cid::default(),
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
        assert!(removed, "Filter should be successfully removed");

        // Verify that the filter is no longer in the event manager
        {
            let filters = event_manager.filters.read();
            assert!(!filters.contains_key(&filter_id));
        }
    }
}
