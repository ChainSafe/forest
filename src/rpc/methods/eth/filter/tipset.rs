// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc::eth::{filter::Filter, filter::FilterManager, FilterID};
use crate::rpc::Arc;
use crate::shim::fvm_shared_latest::clock::ChainEpoch;
use ahash::AHashMap as HashMap;
use anyhow::{Context, Result};
use parking_lot::RwLock;
use std::any::Any;

#[allow(dead_code)]
#[derive(Debug, PartialEq)]
pub struct TipSetFilter {
    pub id: FilterID,
    pub max_results: usize,
    pub collected: ChainEpoch,
}

impl TipSetFilter {
    pub fn new(max_results: usize) -> Result<Arc<Self>, uuid::Error> {
        let id = FilterID::new()?;
        Ok(Arc::new(Self {
            id,
            max_results,
            collected: 0,
        }))
    }
}

impl Filter for TipSetFilter {
    fn id(&self) -> &FilterID {
        &self.id
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// The `TipSetFilterManager` structure maintains a set of filters that operate on TipSets,
/// allowing new filters to be installed or existing ones to be removed. It ensures that each
/// filter is uniquely identifiable by its ID and that a maximum number of results can be
/// configured for each filter.
#[derive(Debug)]
pub struct TipSetFilterManager {
    filters: RwLock<HashMap<FilterID, Arc<dyn Filter>>>,
    max_filter_results: usize,
}

impl TipSetFilterManager {
    pub fn new(max_filter_results: usize) -> Arc<Self> {
        Arc::new(Self {
            filters: RwLock::new(HashMap::new()),
            max_filter_results,
        })
    }
}

impl FilterManager for TipSetFilterManager {
    fn install(&self) -> Result<Arc<dyn Filter>> {
        let filter = TipSetFilter::new(self.max_filter_results)
            .context("Failed to create a new tipset filter")?;
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
    fn test_tipset_filter() {
        // Test case 1: Create a TipSetFilter
        let max_results = 10;
        let filter = TipSetFilter::new(max_results).expect("Failed to create TipSetFilter");
        assert_eq!(filter.max_results, max_results);

        // Test case 2: Create a TipSetFilterManager and install the TipSetFilter
        let tipset_manager = TipSetFilterManager::new(max_results);
        let installed_filter = tipset_manager
            .install()
            .expect("Failed to install TipSetFilter");

        // Verify that the filter has been added to the tipset manager
        {
            let filters = tipset_manager.filters.read();
            assert!(filters.contains_key(installed_filter.id()));
        }

        // Test case 3: Remove the installed TipSetFilter
        let filter_id = installed_filter.id().clone();
        let removed = tipset_manager.remove(&filter_id);
        assert_eq!(
            removed.map(|f| f.id().clone()),
            Some(installed_filter.id().clone()),
            "Filter should be successfully removed"
        );

        // Verify that the filter is no longer in the tipset manager
        {
            let filters = tipset_manager.filters.read();
            assert!(!filters.contains_key(&filter_id));
        }
    }
}
