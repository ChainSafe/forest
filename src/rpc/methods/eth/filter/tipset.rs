// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::FilterError;
use crate::blocks::{Tipset, TipsetKey};
use crate::rpc::eth::{filter::Filter, FilterID};
use crate::rpc::Arc;
use ahash::AHashMap as HashMap;
use parking_lot::Mutex;
use std::any::Any;
use std::time::SystemTime;
use tokio::sync::mpsc::Sender;

#[derive(Debug)]
pub struct TipSetFilter {
    id: FilterID,
    max_results: usize,
    sub_channel: Mutex<Option<Sender<Box<dyn Any + Send>>>>,
    collected: Mutex<Vec<TipsetKey>>,
    last_taken: Mutex<SystemTime>,
}

impl TipSetFilter {
    pub fn new(max_results: usize) -> Result<Arc<Self>, uuid::Error> {
        let id = FilterID::new()?;
        Ok(Arc::new(Self {
            id,
            max_results,
            sub_channel: Mutex::new(None),
            collected: Mutex::new(Vec::new()),
            last_taken: Mutex::new(SystemTime::now()),
        }))
    }
    pub async fn collect_tipset(&self, tipset: &Tipset) {
        let mut collected = self.collected.lock();
        let sub_channel = self.sub_channel.lock();

        if let Some(ref ch) = *sub_channel {
            ch.send(Box::new(tipset.clone())).await.ok();
            return;
        }

        if self.max_results > 0 && collected.len() == self.max_results {
            collected.remove(0);
        }

        collected.push(tipset.key().clone());
    }
    pub fn take_collected_tipsets(&self) -> Vec<TipsetKey> {
        let mut collected = self.collected.lock();
        let mut last_taken = self.last_taken.lock();

        let result = collected.clone();
        collected.clear();
        *last_taken = SystemTime::now();

        result
    }
}

impl Filter for TipSetFilter {
    fn id(&self) -> FilterID {
        self.id.clone()
    }

    fn last_taken(&self) -> SystemTime {
        *self.last_taken.lock()
    }

    fn set_sub_channel(&self, sub_channel: Sender<Box<dyn Any + Send>>) {
        let mut sc = self.sub_channel.lock();
        *sc = Some(sub_channel);
        self.collected.lock().clear();
    }

    fn clear_sub_channel(&self) {
        let mut sc = self.sub_channel.lock();
        *sc = None;
    }

    fn as_any(&self) -> &dyn Any {
        self
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

    pub async fn apply(&self, _from: &Tipset, to: &Tipset) -> Result<(), FilterError> {
        let filters = self.filters.lock();
        for filter in filters.values() {
            filter.collect_tipset(to).await;
        }
        Ok(())
    }

    pub fn revert(&self, _from: &Tipset, _to: &Tipset) -> Result<(), FilterError> {
        Ok(())
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
