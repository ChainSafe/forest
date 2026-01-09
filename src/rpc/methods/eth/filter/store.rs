// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc::Arc;
use crate::rpc::eth::FilterID;
use ahash::AHashMap as HashMap;
use anyhow::Result;
use anyhow::anyhow;
use parking_lot::RwLock;
use std::any::Any;

/// This trait should be implemented by any filter that needs to be identified
/// and managed. It provide methods to retrieve the unique identifier for
/// the filter.
pub trait Filter: Send + Sync + std::fmt::Debug {
    fn id(&self) -> &FilterID;
    fn as_any(&self) -> &dyn Any;
}

/// The `FilterStore` trait provides the necessary interface for storing and managing filters.
pub trait FilterStore: Send + Sync {
    fn add(&self, filter: Arc<dyn Filter>) -> Result<()>;
    fn get(&self, id: &FilterID) -> Result<Arc<dyn Filter>>;
    fn update(&self, filter: Arc<dyn Filter>);
    fn remove(&self, id: &FilterID) -> Option<Arc<dyn Filter>>;
}

#[derive(Debug)]
pub struct MemFilterStore {
    max: usize,
    filters: RwLock<HashMap<FilterID, Arc<dyn Filter>>>,
}

impl MemFilterStore {
    pub fn new(max_filters: usize) -> Arc<Self> {
        Arc::new(Self {
            max: max_filters,
            filters: RwLock::new(HashMap::new()),
        })
    }
}

impl FilterStore for MemFilterStore {
    fn add(&self, filter: Arc<dyn Filter>) -> Result<()> {
        let mut filters = self.filters.write();

        if filters.len() == self.max {
            return Err(anyhow::Error::msg("Maximum number of filters registered"));
        }
        if filters.contains_key(filter.id()) {
            return Err(anyhow::Error::msg("Filter already registered"));
        }
        filters.insert(filter.id().clone(), filter);
        Ok(())
    }

    fn get(&self, id: &FilterID) -> Result<Arc<dyn Filter>> {
        let filters = self.filters.read();
        filters
            .get(id)
            .cloned()
            .ok_or_else(|| anyhow!("filter not found"))
    }

    fn update(&self, filter: Arc<dyn Filter>) {
        let mut filters = self.filters.write();

        filters.insert(filter.id().clone(), filter);
    }

    fn remove(&self, id: &FilterID) -> Option<Arc<dyn Filter>> {
        let mut filters = self.filters.write();
        filters.remove(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rpc::eth::FilterID;
    use std::sync::Arc;

    #[derive(Debug)]
    struct TestFilter {
        id: FilterID,
    }

    impl Filter for TestFilter {
        fn id(&self) -> &FilterID {
            &self.id
        }

        fn as_any(&self) -> &dyn Any {
            self
        }
    }

    #[test]
    fn test_add_filter() {
        let store = MemFilterStore::new(2);

        let filter1 = Arc::new(TestFilter {
            id: FilterID::new().unwrap(),
        });
        let filter2 = Arc::new(TestFilter {
            id: FilterID::new().unwrap(),
        });
        let duplicate_filter = filter1.clone();

        // Test case 1: Add a new filter
        assert!(store.add(filter1.clone()).is_ok());

        // Test case 2: Attempt to add the same filter again, which should fail as duplicate
        assert!(store.add(duplicate_filter.clone()).is_err());

        // Add another filter
        assert!(store.add(filter2.clone()).is_ok());

        // Test case 3: Attempt to add another filter, which should fail due to max filters reached
        let filter3 = Arc::new(TestFilter {
            id: FilterID::new().unwrap(),
        });
        assert!(store.add(filter3.clone()).is_err());
    }
}
