// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc::eth::{filter::Filter, FilterID};
use crate::rpc::Arc;
use ahash::AHashMap as HashMap;
use anyhow::{Context, Result};
use parking_lot::RwLock;

#[allow(dead_code)]
#[derive(Debug)]
pub struct MempoolFilter {
    id: FilterID,
    max_results: usize,
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
}

/// `MempoolFilterManager` uses a `RwLock` to handle concurrent access to a collection of `MempoolFilter`
/// instances, each identified by a `FilterID`. The number of results returned by the filters is capped by `max_filter_results`.
#[derive(Debug)]
pub struct MempoolFilterManager {
    filters: RwLock<HashMap<FilterID, Arc<MempoolFilter>>>,
    max_filter_results: usize,
}

impl MempoolFilterManager {
    pub fn new(max_filter_results: usize) -> Arc<Self> {
        Arc::new(Self {
            filters: RwLock::new(HashMap::new()),
            max_filter_results,
        })
    }

    pub fn install(&self) -> Result<Arc<MempoolFilter>> {
        let filter = MempoolFilter::new(self.max_filter_results)
            .context("Failed to create a new mempool filter")?;
        let id = filter.id.clone();

        let mut filters = self.filters.write();
        filters.insert(id, filter.clone());

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
        assert!(removed, "Filter should be successfully removed");

        // Verify that the filter is no longer in the mempool manager
        {
            let filters = mempool_manager.filters.read();
            assert!(!filters.contains_key(&filter_id));
        }
    }
}