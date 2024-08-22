// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::FilterError;
use crate::rpc::eth::{filter::Filter, FilterID};
use crate::rpc::Arc;
use ahash::AHashMap as HashMap;
use parking_lot::Mutex;

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
    fn id(&self) -> FilterID {
        self.id.clone()
    }
}

#[derive(Debug)]
pub struct TipSetFilterManager {
    max_filter_results: usize,
    filters: Mutex<HashMap<FilterID, Arc<TipSetFilter>>>,
}

impl TipSetFilterManager {
    pub fn new(max_filter_results: usize) -> Arc<Self> {
        Arc::new(Self {
            max_filter_results,
            filters: Mutex::new(HashMap::new()),
        })
    }

    pub fn install(&self) -> Result<Arc<TipSetFilter>, FilterError> {
        let filter = TipSetFilter::new(self.max_filter_results)?;
        let id = filter.id.clone();

        let mut filters = self.filters.lock();
        filters.insert(id, filter.clone());

        Ok(filter)
    }

    pub fn remove(&self, id: &FilterID) -> Result<(), FilterError> {
        let mut filters = self.filters.lock();
        if filters.remove(id).is_none() {
            return Err(FilterError::NotFound);
        }
        Ok(())
    }
}
