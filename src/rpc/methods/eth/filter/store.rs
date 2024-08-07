// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::FilterError;
use crate::rpc::eth::FilterID;
use crate::rpc::mpsc::Sender;
use crate::rpc::Arc;
use ahash::AHashMap as HashMap;
use parking_lot::Mutex;
use std::any::Any;
use std::time::SystemTime;

pub trait Filter: Send + Sync + std::fmt::Debug {
    fn id(&self) -> FilterID;
    fn last_taken(&self) -> SystemTime;
    fn set_sub_channel(&self, sub_channel: Sender<Box<dyn Any + Send>>);
    fn clear_sub_channel(&self);

    fn as_any(&self) -> &dyn Any;
}

pub trait FilterStore: Send + Sync {
    fn add(&self, filter: Arc<dyn Filter>) -> Result<(), FilterError>;
    fn get(&self, id: &FilterID) -> Result<Arc<dyn Filter>, FilterError>;
    fn remove(&self, id: &FilterID) -> Result<(), FilterError>;
    fn not_taken_since(&self, when: SystemTime) -> Vec<Arc<dyn Filter>>;
}

#[derive(Debug)]
pub struct MemFilterStore {
    max: usize,
    filters: Mutex<HashMap<FilterID, Arc<dyn Filter>>>,
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
    fn add(&self, filter: Arc<dyn Filter>) -> Result<(), FilterError> {
        let mut filters = self.filters.lock();

        if filters.len() >= self.max {
            return Err(FilterError::MaxFilters);
        }
        if filters.contains_key(&filter.id()) {
            return Err(FilterError::AlreadyRegistered);
        }
        filters.insert(filter.id(), filter);
        Ok(())
    }

    fn get(&self, id: &FilterID) -> Result<Arc<dyn Filter>, FilterError> {
        let filters = self.filters.lock();
        filters.get(id).cloned().ok_or(FilterError::NotFound)
    }

    fn remove(&self, id: &FilterID) -> Result<(), FilterError> {
        let mut filters = self.filters.lock();

        if filters.remove(id).is_none() {
            return Err(FilterError::NotFound);
        }

        Ok(())
    }

    fn not_taken_since(&self, when: SystemTime) -> Vec<Arc<dyn Filter>> {
        let filters = self.filters.lock();

        filters
            .values()
            .filter(|f| f.last_taken().elapsed().unwrap() > when.elapsed().unwrap())
            .cloned()
            .collect()
    }
}
