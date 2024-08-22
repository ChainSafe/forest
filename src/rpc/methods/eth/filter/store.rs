// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::FilterError;
use crate::rpc::eth::FilterID;
use crate::rpc::Arc;
use ahash::AHashMap as HashMap;
use parking_lot::Mutex;

pub trait Filter: Send + Sync + std::fmt::Debug {
    fn id(&self) -> FilterID;
}

pub trait FilterStore: Send + Sync {
    fn add(&self, filter: Arc<dyn Filter>) -> Result<(), FilterError>;
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
}
