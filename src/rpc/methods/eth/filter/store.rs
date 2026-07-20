// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::prelude::*;
use crate::rpc::eth::FilterID;
use ahash::HashMap;
use anyhow::Result;
use anyhow::anyhow;
use parking_lot::Mutex;
use std::any::Any;
use std::time::Duration;
use tokio::time::Instant;

/// This trait should be implemented by any filter that needs to be identified
/// and managed. It provides methods to retrieve the unique identifier for
/// the filter.
pub trait Filter: Send + Sync + std::fmt::Debug {
    fn id(&self) -> &FilterID;
    fn as_any(&self) -> &dyn Any;
}

/// The `FilterStore` trait provides the necessary interface for storing and managing filters.
pub trait FilterStore: Send + Sync {
    fn add(&self, filter: Arc<dyn Filter>) -> Result<()>;
    /// Looks up a filter, marking it as just-polled for [`FilterStore::remove_expired`].
    fn get(&self, id: &FilterID) -> Result<Arc<dyn Filter>>;
    fn remove(&self, id: &FilterID) -> Option<Arc<dyn Filter>>;
    /// Removes and returns every filter not polled via [`FilterStore::get`] for
    /// longer than `ttl`; a never-polled filter counts from when it was added.
    fn remove_expired(&self, ttl: Duration) -> Vec<Arc<dyn Filter>>;
}

/// A stored filter plus the last time it was polled.
#[derive(Debug)]
struct FilterEntry {
    filter: Arc<dyn Filter>,
    last_used: Instant,
}

#[derive(Debug)]
pub struct MemFilterStore {
    max: usize,
    filters: Mutex<HashMap<FilterID, FilterEntry>>,
}

impl MemFilterStore {
    pub fn new(max_filters: usize) -> Arc<Self> {
        Arc::new(Self {
            max: max_filters,
            filters: Mutex::new(HashMap::new()),
        })
    }
}

impl FilterStore for MemFilterStore {
    fn add(&self, filter: Arc<dyn Filter>) -> Result<()> {
        let mut filters = self.filters.lock();

        if filters.len() == self.max {
            return Err(anyhow::Error::msg("Maximum number of filters registered"));
        }
        if filters.contains_key(filter.id()) {
            return Err(anyhow::Error::msg("Filter already registered"));
        }
        filters.insert(
            filter.id().clone(),
            FilterEntry {
                filter,
                last_used: Instant::now(),
            },
        );
        Ok(())
    }

    fn get(&self, id: &FilterID) -> Result<Arc<dyn Filter>> {
        let mut filters = self.filters.lock();
        let entry = filters
            .get_mut(id)
            .ok_or_else(|| anyhow!("filter not found"))?;
        entry.last_used = Instant::now();
        Ok(entry.filter.clone())
    }

    fn remove(&self, id: &FilterID) -> Option<Arc<dyn Filter>> {
        let mut filters = self.filters.lock();
        filters.remove(id).map(|entry| entry.filter)
    }

    fn remove_expired(&self, ttl: Duration) -> Vec<Arc<dyn Filter>> {
        let now = Instant::now();
        let mut filters = self.filters.lock();
        filters
            .extract_if(|_, entry| now.duration_since(entry.last_used) > ttl)
            .map(|(_, entry)| entry.filter)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rpc::eth::FilterID;
    use std::sync::Arc;

    const TTL: Duration = Duration::from_hours(1);

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

    fn test_filter() -> Arc<TestFilter> {
        Arc::new(TestFilter {
            id: FilterID::new().unwrap(),
        })
    }

    #[tokio::test(start_paused = true)]
    async fn remove_expired_evicts_only_idle_filters() {
        let store = MemFilterStore::new(10);
        let stale = test_filter();
        store.add(stale.clone()).unwrap();

        // the fresh filter is installed one TTL later
        tokio::time::advance(TTL + Duration::from_secs(1)).await;
        let fresh = test_filter();
        store.add(fresh.clone()).unwrap();

        let expired = store.remove_expired(TTL);
        assert_eq!(
            expired.iter().map(|f| f.id().clone()).collect::<Vec<_>>(),
            vec![stale.id().clone()]
        );
        assert!(store.get(stale.id()).is_err(), "stale filter is gone");
        assert!(store.get(fresh.id()).is_ok(), "fresh filter survives");
    }

    #[tokio::test(start_paused = true)]
    async fn get_bumps_last_used_and_keeps_the_filter_alive() {
        let store = MemFilterStore::new(10);
        let filter = test_filter();
        store.add(filter.clone()).unwrap();

        // polling after 45 minutes resets the idle clock...
        tokio::time::advance(Duration::from_mins(45)).await;
        store.get(filter.id()).unwrap();

        // ...so 45 more minutes later (90 since creation) it is not expired
        tokio::time::advance(Duration::from_mins(45)).await;
        assert!(store.remove_expired(TTL).is_empty());

        // left unpolled past the TTL, it expires
        tokio::time::advance(TTL + Duration::from_secs(1)).await;
        assert_eq!(store.remove_expired(TTL).len(), 1);
    }

    #[test]
    fn test_add_filter() {
        let store = MemFilterStore::new(2);

        let filter1 = test_filter();
        let filter2 = test_filter();
        let duplicate_filter = filter1.clone();

        // Test case 1: Add a new filter
        assert!(store.add(filter1.clone()).is_ok());

        // Test case 2: Attempt to add the same filter again, which should fail as duplicate
        assert!(store.add(duplicate_filter.clone()).is_err());

        // Add another filter
        assert!(store.add(filter2.clone()).is_ok());

        // Test case 3: Attempt to add another filter, which should fail due to max filters reached
        let filter3 = test_filter();
        assert!(store.add(filter3.clone()).is_err());
    }
}
