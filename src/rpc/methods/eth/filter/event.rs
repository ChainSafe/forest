// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::FilterError;
use crate::rpc::eth::filter::ActorEventBlock;
use crate::rpc::eth::filter::ParsedFilter;
use crate::rpc::eth::{filter::Filter, FilterID};
use crate::rpc::Arc;
use crate::shim::address::Address;
use crate::shim::clock::ChainEpoch;
use ahash::AHashMap as HashMap;
use cid::Cid;
use parking_lot::Mutex;

#[allow(dead_code)]
#[derive(Debug)]
pub struct EventFilter {
    id: FilterID,
    min_height: ChainEpoch,
    max_height: ChainEpoch,
    tipset_cid: Cid,
    addresses: Vec<Address>,
    keys_with_codec: HashMap<String, Vec<ActorEventBlock>>,
    max_results: usize,
}

impl Filter for EventFilter {
    fn id(&self) -> FilterID {
        self.id.clone()
    }
}

pub struct EventFilterManager {
    filters: Mutex<HashMap<FilterID, Arc<EventFilter>>>,
    max_filter_results: usize,
}

impl EventFilterManager {
    pub fn new(max_filter_results: usize) -> Arc<Self> {
        Arc::new(Self {
            filters: Mutex::new(HashMap::new()),
            max_filter_results,
        })
    }

    pub fn install(&self, pf: ParsedFilter) -> Result<Arc<dyn Filter>, FilterError> {
        let id = FilterID::new()?;

        let filter = Arc::new(EventFilter {
            id: id.clone(),
            min_height: pf.min_height,
            max_height: pf.max_height,
            tipset_cid: pf.tipset_cid,
            addresses: pf.addresses,
            keys_with_codec: pf.keys,
            max_results: self.max_filter_results,
        });

        self.filters.lock().insert(id, filter.clone());

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
