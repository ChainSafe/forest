// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::prelude::*;
use crate::rpc::eth::{FilterID, filter::Filter, filter::FilterManager};
use crate::shim::fvm_shared_latest::clock::ChainEpoch;
use ahash::HashMap;
use anyhow::Result;
use parking_lot::{Mutex, RwLock};
use std::any::Any;

#[derive(Debug)]
pub struct TipSetFilter {
    // Unique id used to identify the filter
    pub id: FilterID,
    // Epoch at which the results were collected
    collected: Mutex<Option<ChainEpoch>>,
}

impl TipSetFilter {
    pub fn new() -> Result<Arc<Self>, uuid::Error> {
        let id = FilterID::new()?;
        Ok(Arc::new(Self {
            id,
            collected: Mutex::new(None),
        }))
    }

    /// Epoch recorded by the last poll that found events.
    pub fn collected(&self) -> Option<ChainEpoch> {
        *self.collected.lock()
    }

    /// Records the highest epoch seen by a poll.
    pub fn set_collected(&self, epoch: ChainEpoch) {
        *self.collected.lock() = Some(epoch);
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
/// filter is uniquely identifiable by its ID.
#[derive(Debug)]
pub struct TipSetFilterManager {
    filters: RwLock<HashMap<FilterID, Arc<dyn Filter>>>,
}

impl TipSetFilterManager {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            filters: RwLock::new(HashMap::new()),
        })
    }
}

impl FilterManager for TipSetFilterManager {
    fn install(&self) -> Result<Arc<dyn Filter>> {
        let filter = TipSetFilter::new().context("Failed to create a new tipset filter")?;
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
        // Test case 1: Create a TipSetFilterManager and install a TipSetFilter
        let tipset_manager = TipSetFilterManager::new();
        let installed_filter = tipset_manager
            .install()
            .expect("Failed to install TipSetFilter");

        // Verify that the filter has been added to the tipset manager
        {
            let filters = tipset_manager.filters.read();
            assert!(filters.contains_key(installed_filter.id()));
        }

        // Test case 2: Remove the installed TipSetFilter
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
