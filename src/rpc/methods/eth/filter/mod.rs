// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod event;
mod mempool;
mod store;
mod tipset;

use crate::rpc::eth::filter::event::*;
use crate::rpc::eth::filter::mempool::*;
use crate::rpc::eth::filter::tipset::*;
use crate::rpc::eth::types::*;
use crate::shim::address::Address;
use crate::shim::clock::ChainEpoch;
use crate::utils::misc::env::env_or_default;
use ahash::AHashMap as HashMap;
use cid::Cid;
use serde::*;
use std::sync::Arc;
use store::*;
use thiserror::Error;

#[derive(Error, Debug, Clone)]
pub enum FilterError {
    #[error("filter already registered")]
    AlreadyRegistered,
    #[error("filter not found")]
    NotFound,
    #[error("maximum number of filters registered")]
    MaxFilters,
    #[error("uuid generation error")]
    UuidError(#[from] uuid::Error),
    #[error("Not supported")]
    NotSupported,
    #[error("Parsing error: {0}")]
    ParsingError(String),
    #[error("Installation error: {0}")]
    InstallationError(String),
    #[error("Removal error: {0}")]
    RemovalError(String),
}

pub struct EthEventHandler {
    filter_store: Option<Arc<dyn FilterStore>>,
    max_filter_height_range: ChainEpoch,
    event_filter_manager: Option<Arc<EventFilterManager>>,
    tipset_filter_manager: Option<Arc<TipSetFilterManager>>,
    mempool_filter_manager: Option<Arc<MemPoolFilterManager>>,
}

impl EthEventHandler {
    pub fn new() -> Self {
        let max_filters: usize = env_or_default("MAX_FILTERS", 100);
        let max_filter_results: usize = env_or_default("MAX_FILTER_RESULTS", 10000);
        let max_filter_height_range: i64 = env_or_default("MAX_FILTER_HEIGHT_RANGE", 2880);
        let filter_store: Option<Arc<dyn FilterStore>> =
            Some(MemFilterStore::new(max_filters) as Arc<dyn FilterStore>);
        let event_filter_manager = Some(EventFilterManager::new(max_filter_results));
        let tipset_filter_manager = Some(TipSetFilterManager::new(max_filter_results));
        let mempool_filter_manager = Some(MemPoolFilterManager::new(max_filter_results));

        Self {
            filter_store,
            max_filter_height_range,
            event_filter_manager,
            tipset_filter_manager,
            mempool_filter_manager,
        }
    }

    pub fn eth_new_filter(
        &self,
        filter_spec: &EthFilterSpec,
        chain_height: i64,
    ) -> Result<FilterID, FilterError> {
        if self.filter_store.is_none() || self.event_filter_manager.is_none() {
            return Err(FilterError::NotSupported);
        }

        let pf = filter_spec
            .parse_eth_filter_spec(chain_height, self.max_filter_height_range)
            .map_err(FilterError::ParsingError)?;

        let f = self
            .event_filter_manager
            .as_ref()
            .unwrap()
            .install(
                pf.min_height,
                pf.max_height,
                pf.tipset_cid,
                pf.addresses,
                pf.keys,
                true,
            )
            .map_err(|e| FilterError::InstallationError(e.to_string()))?;

        self.filter_store
            .as_ref()
            .unwrap()
            .add(f.clone())
            .map_err(|e| {
                self.tipset_filter_manager
                    .as_ref()
                    .unwrap()
                    .remove(&f.id())
                    .unwrap_or(());
                FilterError::RemovalError(e.to_string())
            })?;

        Ok(f.id())
    }

    pub fn eth_new_block_filter(&self) -> Result<FilterID, FilterError> {
        if self.filter_store.is_none() || self.tipset_filter_manager.is_none() {
            return Err(FilterError::NotSupported);
        }

        let manager = self.tipset_filter_manager.as_ref().unwrap();
        let filter = manager
            .install()
            .map_err(|e| FilterError::InstallationError(e.to_string()))?;

        if let Err(err) = self.filter_store.as_ref().unwrap().add(filter.clone()) {
            let removal_error = manager.remove(&filter.id());
            if let Err(err2) = removal_error {
                return Err(FilterError::RemovalError(format!(
                    "encountered error {:?} while removing new filter due to {:?}",
                    err2, err
                )));
            }
            return Err(FilterError::InstallationError(err.to_string()));
        }

        Ok(filter.id())
    }

    pub fn eth_new_pending_transaction_filter(&self) -> Result<FilterID, FilterError> {
        if self.filter_store.is_none() || self.mempool_filter_manager.is_none() {
            return Err(FilterError::NotSupported);
        }

        let manager = self.mempool_filter_manager.as_ref().unwrap();
        let filter = manager
            .install()
            .map_err(|e| FilterError::InstallationError(e.to_string()))?;

        if let Err(err) = self.filter_store.as_ref().unwrap().add(filter.clone()) {
            let removal_error = manager.remove(&filter.id());
            if let Err(err2) = removal_error {
                return Err(FilterError::RemovalError(format!(
                    "encountered error {:?} while removing new filter due to {:?}",
                    err2, err
                )));
            }
            return Err(FilterError::InstallationError(err.to_string()));
        }

        Ok(filter.id())
    }

    pub fn eth_uninstall_filter(&self, id: &FilterID) -> Result<bool, FilterError> {
        if self.filter_store.is_none() {
            return Err(FilterError::NotSupported);
        }

        let store = self.filter_store.as_ref().unwrap();
        let filter = store.get(id);

        match filter {
            Ok(f) => {
                self.uninstall_filter(f)?;
                Ok(true)
            }
            Err(_) => Ok(false),
        }
    }

    fn uninstall_filter(&self, filter: Arc<dyn Filter>) -> Result<(), FilterError> {
        let id = filter.id();

        let result = if filter.as_any().is::<EventFilter>() {
            self.event_filter_manager.as_ref().unwrap().remove(&id)
        } else if filter.as_any().is::<TipSetFilter>() {
            self.tipset_filter_manager.as_ref().unwrap().remove(&id)
        } else if filter.as_any().is::<MemPoolFilter>() {
            self.mempool_filter_manager.as_ref().unwrap().remove(&id)
        } else {
            Err(FilterError::NotFound)
        };

        result.map_err(|e| FilterError::RemovalError(e.to_string()))?;

        self.filter_store
            .as_ref()
            .unwrap()
            .remove(&id)
            .map_err(|e| FilterError::RemovalError(e.to_string()))
    }
}

impl EthFilterSpec {
    fn parse_eth_filter_spec(
        &self,
        chain_height: i64,
        max_filter_height_range: i64,
    ) -> Result<ParsedFilter, String> {
        let mut min_height = 0;
        let mut max_height = 0;
        let mut tipset_cid = Cid::default();
        let mut addresses = Vec::new();
        if let Some(block_hash) = &self.block_hash {
            if self.from_block.is_some() || self.to_block.is_some() {
                return Err("must not specify block hash and from/to block".to_string());
            }

            tipset_cid = Cid::try_from(block_hash.0.as_bytes()).map_err(|e| e.to_string())?;
        } else {
            let (min, max) = parse_block_range(
                chain_height,
                self.from_block.as_deref(),
                self.to_block.as_deref(),
                max_filter_height_range,
            )?;
            min_height = min;
            max_height = max;
        }

        for ea in &self.address {
            let addr = ea
                .to_filecoin_address()
                .map_err(|e| format!("invalid address {}", e))?;
            addresses.push(addr);
        }

        let keys = parse_eth_topics(&self.topics)?;

        Ok(ParsedFilter {
            min_height,
            max_height,
            tipset_cid,
            addresses,
            keys: keys_to_keys_with_codec(keys),
        })
    }
}

fn parse_block_range(
    heaviest: ChainEpoch,
    from_block: Option<&str>,
    to_block: Option<&str>,
    max_range: ChainEpoch,
) -> Result<(ChainEpoch, ChainEpoch), String> {
    let min_height = match from_block {
        None | Some("latest") | Some("") => heaviest,
        Some("earliest") => 0,
        Some(block) => {
            if !block.starts_with("0x") {
                return Err("FromBlock is not a hex".to_string());
            }
            hex_str_to_epoch(block).map_err(|_| "invalid epoch".to_string())?
        }
    };

    let max_height = match to_block {
        None | Some("latest") | Some("") => -1,
        Some("earliest") => 0,
        Some(block) => {
            if !block.starts_with("0x") {
                return Err("ToBlock is not a hex".to_string());
            }
            hex_str_to_epoch(block).map_err(|_| "invalid epoch".to_string())?
        }
    };

    if min_height == -1 && max_height > 0 {
        if max_height - heaviest > max_range {
            return Err(format!(
                "invalid epoch range: to block is too far in the future (maximum: {})",
                max_range
            ));
        }
    } else if min_height >= 0 && max_height == -1 {
        if heaviest - min_height > max_range {
            return Err(format!(
                "invalid epoch range: from block is too far in the past (maximum: {})",
                max_range
            ));
        }
    } else if min_height >= 0 && max_height >= 0 {
        if min_height > max_height {
            return Err(format!(
                "invalid epoch range: to block ({}) must be after from block ({})",
                max_height, min_height
            ));
        } else if max_height - min_height > max_range {
            return Err(format!(
                "invalid epoch range: range between to and from blocks is too large (maximum: {})",
                max_range
            ));
        }
    }

    Ok((min_height, max_height))
}

fn hex_str_to_epoch(hex_str: &str) -> Result<ChainEpoch, String> {
    let hex_substring = hex_str
        .get(2..)
        .ok_or_else(|| "invalid hex string: unable to parse epoch".to_string())?;
    i64::from_str_radix(hex_substring, 16).map_err(|e| e.to_string())
}

fn parse_eth_topics(topics: &EthTopicSpec) -> Result<HashMap<String, Vec<Vec<u8>>>, String> {
    let mut keys: HashMap<String, Vec<Vec<u8>>> = HashMap::new();
    for (idx, vals) in topics.0.iter().enumerate() {
        if vals.0.is_empty() {
            continue;
        }
        let key = format!("t{}", idx + 1);
        for v in &vals.0 {
            keys.entry(key.clone()).or_default().push(v.0 .0.to_vec());
        }
    }
    Ok(keys)
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct ActorEventBlock {
    codec: u64,
    value: Vec<u8>,
}

const MULTICODEC_RAW: u64 = 0x55;

fn keys_to_keys_with_codec(
    keys: HashMap<String, Vec<Vec<u8>>>,
) -> HashMap<String, Vec<ActorEventBlock>> {
    let mut keys_with_codec: HashMap<String, Vec<ActorEventBlock>> = HashMap::new();

    for (k, v) in keys {
        for vv in v {
            keys_with_codec
                .entry(k.clone())
                .or_default()
                .push(ActorEventBlock {
                    codec: MULTICODEC_RAW,
                    value: vv,
                });
        }
    }

    keys_with_codec
}

struct ParsedFilter {
    min_height: ChainEpoch,
    max_height: ChainEpoch,
    tipset_cid: Cid,
    addresses: Vec<Address>,
    keys: HashMap<String, Vec<ActorEventBlock>>,
}

#[cfg(test)]
mod tests {
    use crate::rpc::eth::filter::EthEventHandler;
    use crate::rpc::eth::types::*;
    use std::str::FromStr;

    #[test]
    fn test_eth_uninstall_filter_using_event_handler() {
        let event_handler = EthEventHandler::new();
        let mut filter_ids = Vec::new();
        let filter_spec = EthFilterSpec {
            from_block: None,
            to_block: None,
            address: vec![
                EthAddress::from_str("0xff38c072f286e3b20b3954ca9f99c05fbecc64aa").unwrap(),
            ],
            topics: EthTopicSpec(vec![]),
            block_hash: None,
        };

        let filter_id = event_handler.eth_new_filter(&filter_spec, 0).unwrap();
        filter_ids.push(filter_id);

        let block_filter_id = event_handler.eth_new_block_filter().unwrap();
        filter_ids.push(block_filter_id);

        let pending_tx_filter_id = event_handler.eth_new_pending_transaction_filter().unwrap();
        filter_ids.push(pending_tx_filter_id);

        for filter_id in filter_ids {
            let result = event_handler.eth_uninstall_filter(&filter_id).unwrap();
            assert_eq!(
                result, true,
                "Uninstalling filter with id {:?} failed",
                &filter_id
            );
        }
    }
}
