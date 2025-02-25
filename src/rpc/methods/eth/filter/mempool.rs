// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc::Arc;
use crate::rpc::eth::{FilterID, filter::Filter, filter::FilterManager};
use ahash::AHashMap as HashMap;
use anyhow::{Context, Result};
use parking_lot::RwLock;
use std::any::Any;

/// Data structure for filtering and collecting pending transactions
/// from the mempool before they are confirmed in a block.
#[allow(dead_code)]
#[derive(Debug, PartialEq)]
pub struct MempoolFilter {
    id: FilterID,       // Unique id used to identify the filter
    max_results: usize, // maximum number of results to collect
}

impl MempoolFilter {
    pub fn new(max_results: usize) -> Result<Arc<Self>, uuid::Error> {
        let id = FilterID::new()?;
        Ok(Arc::new(Self { id, max_results }))
    }
}

impl Filter for MempoolFilter {
    fn id(&self) -> &FilterID {
        &self.id
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// `MempoolFilterManager` uses a `RwLock` to handle concurrent access to a collection of `MempoolFilter`
/// instances, each identified by a `FilterID`. The number of results returned by the filters is capped by `max_filter_results`.
#[derive(Debug)]
pub struct MempoolFilterManager {
    filters: RwLock<HashMap<FilterID, Arc<dyn Filter>>>,
    max_filter_results: usize,
}

impl MempoolFilterManager {
    pub fn new(max_filter_results: usize) -> Arc<Self> {
        Arc::new(Self {
            filters: RwLock::new(HashMap::new()),
            max_filter_results,
        })
    }
}

impl FilterManager for MempoolFilterManager {
    fn install(&self) -> Result<Arc<dyn Filter>> {
        let filter = MempoolFilter::new(self.max_filter_results)
            .context("Failed to create a new mempool filter")?;
        let id = filter.id().clone();

        self.filters.write().insert(id, filter.clone());

        Ok(filter)
    }

    fn remove(&self, id: &FilterID) -> Option<Arc<dyn Filter>> {
        let mut filters = self.filters.write();
        filters.remove(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mempool_filter() {
        // Test case 1: Create a mempool filter
        let max_results = 10;
        let filter = MempoolFilter::new(max_results).expect("Failed to create mempool filter");
        assert_eq!(filter.max_results, max_results);

        // Test case 2: Create a mempool filter manager and install the mempool filter
        let mempool_manager = MempoolFilterManager::new(max_results);
        let installed_filter = mempool_manager
            .install()
            .expect("Failed to install mempool filter");

        // Verify that the filter has been added to the mempool manager
        {
            let filters = mempool_manager.filters.read();
            assert!(filters.contains_key(installed_filter.id()));
        }

        // Test case 3: Remove the installed mempool filter
        let filter_id = installed_filter.id().clone();
        let removed = mempool_manager.remove(&filter_id);
        assert_eq!(
            removed.map(|f| f.id().clone()),
            Some(installed_filter.id().clone()),
            "Filter should be successfully removed"
        );

        // Verify that the filter is no longer in the mempool manager
        {
            let filters = mempool_manager.filters.read();
            assert!(!filters.contains_key(&filter_id));
        }
    }
}
