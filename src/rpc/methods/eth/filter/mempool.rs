// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::FilterError;
use crate::message::SignedMessage;
use crate::rpc::eth::{filter::Filter, FilterID};
use crate::rpc::Arc;
use ahash::AHashMap as HashMap;
use parking_lot::Mutex;
use std::any::Any;
use std::time::SystemTime;
use tokio::sync::mpsc::Sender;

#[derive(Debug)]
pub struct MemPoolFilter {
    id: FilterID,
    max_results: usize,
    sub_channel: Mutex<Option<Sender<Box<dyn Any + Send>>>>,
    collected: Mutex<Vec<SignedMessage>>,
    last_taken: Mutex<SystemTime>,
}

impl MemPoolFilter {
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

    pub async fn collect_message(&self, message: SignedMessage) {
        let sub_channel_opt = {
            let sub_channel = self.sub_channel.lock();
            sub_channel.clone()
        };
        if let Some(ref ch) = sub_channel_opt {
            ch.send(Box::new(message.clone())).await.ok();
            return;
        }
        let mut collected = self.collected.lock();
        if self.max_results > 0 && collected.len() == self.max_results {
            collected.remove(0);
        }

        collected.push(message);
    }

    pub fn take_collected_messages(&self) -> Vec<SignedMessage> {
        let mut collected = self.collected.lock();
        let mut last_taken = self.last_taken.lock();

        let result = collected.clone();
        collected.clear();
        *last_taken = SystemTime::now();

        result
    }
}

impl Filter for MemPoolFilter {
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
pub struct MemPoolFilterManager {
    max_filter_results: usize,
    filters: Mutex<HashMap<FilterID, Arc<MemPoolFilter>>>,
}

impl MemPoolFilterManager {
    pub fn new(max_filter_results: usize) -> Arc<Self> {
        Arc::new(Self {
            max_filter_results,
            filters: Mutex::new(HashMap::new()),
        })
    }

    pub async fn process_update(&self, message: SignedMessage) {
        let filters = {
            let filters_guard = self.filters.lock();
            filters_guard.clone()
        };
        for filter in filters.values() {
            filter.collect_message(message.clone()).await;
        }
    }

    pub fn install(&self) -> Result<Arc<MemPoolFilter>, FilterError> {
        let filter = MemPoolFilter::new(self.max_filter_results)?;
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
