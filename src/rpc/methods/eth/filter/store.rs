// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc::eth::FilterID;
use crate::rpc::Arc;
use ahash::AHashMap as HashMap;
use anyhow::Result;
use parking_lot::RwLock;

pub trait Filter: Send + Sync + std::fmt::Debug {
    fn id(&self) -> &FilterID;
}

pub trait FilterStore: Send + Sync {
    fn add(&self, filter: Arc<dyn Filter>) -> Result<()>;
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

        if filters.len() >= self.max {
            return Err(anyhow::Error::msg("Maximum number of filters registered"));
        }
        if filters.contains_key(filter.id()) {
            return Err(anyhow::Error::msg("Filter already registered"));
        }
        filters.insert(filter.id().clone(), filter);
        Ok(())
    }
}
