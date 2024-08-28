// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc::eth::{filter::Filter, FilterID};
use crate::rpc::Arc;
use ahash::AHashMap as HashMap;
use anyhow::{Context, Result};
use parking_lot::RwLock;

#[allow(dead_code)]
#[derive(Debug)]
pub struct TipSetFilter {
    id: FilterID,
    max_results: usize,
}

impl TipSetFilter {
    pub fn new(max_results: usize) -> Result<Arc<Self>, uuid::Error> {
        let id = FilterID::new()?;
        Ok(Arc::new(Self { id, max_results }))
    }
}

impl Filter for TipSetFilter {
    fn id(&self) -> &FilterID {
        &self.id
    }
}

#[derive(Debug)]
pub struct TipSetFilterManager {
    filters: RwLock<HashMap<FilterID, Arc<TipSetFilter>>>,
    max_filter_results: usize,
}

impl TipSetFilterManager {
    pub fn new(max_filter_results: usize) -> Arc<Self> {
        Arc::new(Self {
            filters: RwLock::new(HashMap::new()),
            max_filter_results,
        })
    }

    pub fn install(&self) -> Result<Arc<TipSetFilter>> {
        let filter = TipSetFilter::new(self.max_filter_results)
            .context("Failed to create a new TipSetFilter")?;
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
