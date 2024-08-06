// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::blocks::TipsetKey;
use crate::lotus_json::lotus_json_with_self;
use crate::message::SignedMessage;
use crate::rpc::eth::EthAddress;
use crate::shim::clock::ChainEpoch;
use crate::utils::misc::env::env_or_default;
use ahash::AHashMap as HashMap;
use cid::Cid;
use keccak_hash::H256;
use parking_lot::Mutex;
use schemars::JsonSchema;
use serde::*;
use std::any::Any;
use std::sync::{mpsc::Sender, Arc};
use std::time::SystemTime;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Hash, Clone)]
pub struct FilterID(EthHash);

lotus_json_with_self!(FilterID);

impl FilterID {
    fn new() -> Result<Self, uuid::Error> {
        let raw_id = Uuid::new_v4();
        let mut id = [0u8; 16];
        id.copy_from_slice(raw_id.as_bytes());
        Ok(FilterID(EthHash(H256::from_slice(&id))))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Hash)]
pub struct EthHash(#[schemars(with = "String")] H256);

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema)]
pub struct EthHashList(Vec<EthHash>);

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema)]
pub struct EthTopicSpec(pub Vec<EthHashList>);

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EthFilterSpec {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_block: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to_block: Option<String>,
    pub address: Vec<EthAddress>,
    pub topics: EthTopicSpec,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block_hash: Option<EthHash>,
}

lotus_json_with_self!(EthFilterSpec);

#[derive(Error, Debug)]
pub enum EthError {
    #[error("Not Supported")]
    NotSupported,
    #[error("Parsing Error: {0}")]
    ParsingError(String),
    #[error("Installation Error: {0}")]
    InstallationError(String),
    #[error("Removal Error: {0}")]
    RemovalError(String),
}

pub trait Filter: Send + Sync + std::fmt::Debug {
    fn id(&self) -> FilterID;
    #[allow(dead_code)]
    fn last_taken(&self) -> SystemTime;
    #[allow(dead_code)]
    fn set_sub_channel(&self, sub_channel: Sender<Box<dyn Any + Send>>);
    #[allow(dead_code)]
    fn clear_sub_channel(&self);

    fn as_any(&self) -> &dyn Any;
}

pub trait FilterStore: Send + Sync {
    fn add(&self, filter: Arc<dyn Filter>) -> Result<(), &'static str>;
    fn get(&self, id: &FilterID) -> Result<Arc<dyn Filter>, &'static str>;
    fn remove(&self, id: &FilterID) -> Result<(), &'static str>;
    #[allow(dead_code)]
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
    fn add(&self, filter: Arc<dyn Filter>) -> Result<(), &'static str> {
        let mut filters = self.filters.lock();

        if filters.len() >= self.max {
            return Err("maximum number of filters registered");
        }

        if filters.contains_key(&filter.id()) {
            return Err("filter already registered");
        }

        filters.insert(filter.id(), filter);
        Ok(())
    }

    fn get(&self, id: &FilterID) -> Result<Arc<dyn Filter>, &'static str> {
        let filters = self.filters.lock();
        filters.get(id).cloned().ok_or("filter not found")
    }

    fn remove(&self, id: &FilterID) -> Result<(), &'static str> {
        let mut filters = self.filters.lock();

        if filters.remove(id).is_none() {
            return Err("filter not found");
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
    ) -> Result<FilterID, EthError> {
        if self.filter_store.is_none() || self.event_filter_manager.is_none() {
            return Err(EthError::NotSupported);
        }

        let pf = filter_spec
            .parse_eth_filter_spec(chain_height, self.max_filter_height_range)
            .map_err(EthError::ParsingError)?;

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
            .map_err(|e| EthError::InstallationError(e.to_string()))?;

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
                EthError::RemovalError(e.to_string())
            })?;

        Ok(f.id())
    }

    pub fn eth_new_block_filter(&self) -> Result<FilterID, EthError> {
        if self.filter_store.is_none() || self.tipset_filter_manager.is_none() {
            return Err(EthError::NotSupported);
        }

        let manager = self.tipset_filter_manager.as_ref().unwrap();
        let filter = manager
            .install()
            .map_err(|e| EthError::InstallationError(e.to_string()))?;

        if let Err(err) = self.filter_store.as_ref().unwrap().add(filter.clone()) {
            let removal_error = manager.remove(&filter.id());
            if let Err(err2) = removal_error {
                return Err(EthError::RemovalError(format!(
                    "encountered error {:?} while removing new filter due to {:?}",
                    err2, err
                )));
            }
            return Err(EthError::InstallationError(err.to_string()));
        }

        Ok(filter.id())
    }

    pub fn eth_new_pending_transaction_filter(&self) -> Result<FilterID, EthError> {
        if self.filter_store.is_none() || self.mempool_filter_manager.is_none() {
            return Err(EthError::NotSupported);
        }

        let manager = self.mempool_filter_manager.as_ref().unwrap();
        let filter = manager
            .install()
            .map_err(|e| EthError::InstallationError(e.to_string()))?;

        if let Err(err) = self.filter_store.as_ref().unwrap().add(filter.clone()) {
            let removal_error = manager.remove(&filter.id());
            if let Err(err2) = removal_error {
                return Err(EthError::RemovalError(format!(
                    "encountered error {:?} while removing new filter due to {:?}",
                    err2, err
                )));
            }
            return Err(EthError::InstallationError(err.to_string()));
        }

        Ok(filter.id())
    }

    pub fn eth_uninstall_filter(&self, id: FilterID) -> Result<bool, EthError> {
        if self.filter_store.is_none() {
            return Err(EthError::NotSupported);
        }

        let store = self.filter_store.as_ref().unwrap();
        let filter = store.get(&id);

        match filter {
            Ok(f) => {
                self.uninstall_filter(f)?;
                Ok(true)
            }
            Err(_) => Ok(false),
        }
    }

    fn uninstall_filter(&self, filter: Arc<dyn Filter>) -> Result<(), EthError> {
        let id = filter.id();

        let result = if filter.as_any().is::<EventFilter>() {
            self.event_filter_manager.as_ref().unwrap().remove(&id)
        } else if filter.as_any().is::<TipSetFilter>() {
            self.tipset_filter_manager.as_ref().unwrap().remove(&id)
        } else if filter.as_any().is::<MemPoolFilter>() {
            self.mempool_filter_manager.as_ref().unwrap().remove(&id)
        } else {
            Err("unknown filter type".to_string())
        };

        result.map_err(|e| EthError::RemovalError(e.to_string()))?;

        self.filter_store
            .as_ref()
            .unwrap()
            .remove(&id)
            .map_err(|e| EthError::RemovalError(e.to_string()))
    }
}

pub struct EventFilterManager {
    filters: Mutex<HashMap<FilterID, Arc<dyn Filter>>>,
    max_filter_results: usize,
}

impl EventFilterManager {
    pub fn new(max_filter_results: usize) -> Arc<Self> {
        Arc::new(Self {
            filters: Mutex::new(HashMap::new()),
            max_filter_results,
        })
    }

    pub fn install(
        &self,
        min_height: ChainEpoch,
        max_height: ChainEpoch,
        tipset_cid: Option<Cid>,
        addresses: Vec<String>,
        keys_with_codec: HashMap<String, Vec<ActorEventBlock>>,
        _exclude_reverted: bool,
    ) -> Result<Arc<dyn Filter>, String> {
        let id = FilterID::new().map_err(|e| e.to_string())?;

        let filter = Arc::new(EventFilter {
            id: id.clone(),
            min_height,
            max_height,
            tipset_cid,
            addresses,
            keys_with_codec,
            max_results: self.max_filter_results,
            collected: Mutex::new(Vec::new()),
            last_taken: Mutex::new(SystemTime::now()),
            sub_channel: Mutex::new(None),
        });

        self.filters.lock().insert(id, filter.clone());

        Ok(filter)
    }

    pub fn remove(&self, id: &FilterID) -> Result<(), String> {
        self.filters
            .lock()
            .remove(id)
            .ok_or_else(|| "filter not found".to_string())?;
        Ok(())
    }
}

#[allow(dead_code)]
#[derive(Debug)]
struct EventFilter {
    id: FilterID,
    min_height: ChainEpoch,
    max_height: ChainEpoch,
    tipset_cid: Option<Cid>,
    addresses: Vec<String>,
    keys_with_codec: HashMap<String, Vec<ActorEventBlock>>,
    max_results: usize,
    collected: Mutex<Vec<CollectedEvent>>,
    last_taken: Mutex<SystemTime>,
    sub_channel: Mutex<Option<Sender<Box<dyn Any + Send>>>>,
}

impl Filter for EventFilter {
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

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct CollectedEvent {
    entries: Vec<u8>,
    emitter_addr: String,
    event_idx: usize,
    reverted: bool,
    height: ChainEpoch,
    tipset_key: TipsetKey,
    msg_idx: usize,
    msg_cid: Cid,
}

impl EthFilterSpec {
    fn parse_eth_filter_spec(
        &self,
        chain_height: i64,
        max_filter_height_range: i64,
    ) -> Result<ParsedFilter, String> {
        let mut min_height = 0;
        let mut max_height = 0;
        let mut tipset_cid = None;
        let mut addresses = Vec::new();
        if let Some(block_hash) = &self.block_hash {
            if self.from_block.is_some() || self.to_block.is_some() {
                return Err("must not specify block hash and from/to block".to_string());
            }

            tipset_cid = Some(Cid::try_from(block_hash.0.as_bytes()).map_err(|e| e.to_string())?);
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
            let a = ea
                .to_filecoin_address()
                .map_err(|e| format!("invalid address {}", e))?;
            addresses.push(a.to_string());
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
    tipset_cid: Option<Cid>,
    addresses: Vec<String>,
    keys: HashMap<String, Vec<ActorEventBlock>>,
}

#[derive(Debug)]
pub struct TipSetFilter {
    id: FilterID,
    #[allow(dead_code)]
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
    #[allow(dead_code)]
    pub fn collect_tipset(&self, tipset_key: &TipsetKey) {
        let mut collected = self.collected.lock();
        let sub_channel = self.sub_channel.lock();

        if let Some(ref ch) = *sub_channel {
            ch.send(Box::new(tipset_key.clone())).ok();
            return;
        }

        if self.max_results > 0 && collected.len() == self.max_results {
            collected.remove(0);
        }

        collected.push(tipset_key.clone());
    }
    #[allow(dead_code)]
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
    #[allow(dead_code)]
    pub fn apply(&self, tipset_key: &TipsetKey) {
        let filters = self.filters.lock();
        for filter in filters.values() {
            filter.collect_tipset(tipset_key);
        }
    }

    pub fn install(&self) -> Result<Arc<TipSetFilter>, String> {
        let filter = TipSetFilter::new(self.max_filter_results).map_err(|e| e.to_string())?;
        let id = filter.id.clone();

        let mut filters = self.filters.lock();
        filters.insert(id, filter.clone());

        Ok(filter)
    }

    pub fn remove(&self, id: &FilterID) -> Result<(), String> {
        let mut filters = self.filters.lock();
        filters
            .remove(id)
            .ok_or_else(|| "filter not found".to_string())?;
        Ok(())
    }
}

#[derive(Debug)]
pub struct MemPoolFilter {
    id: FilterID,
    #[allow(dead_code)]
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
    #[allow(dead_code)]
    pub fn collect_message(&self, message: SignedMessage) {
        let mut collected = self.collected.lock();
        let sub_channel = self.sub_channel.lock();

        if let Some(ref ch) = *sub_channel {
            ch.send(Box::new(message.clone())).ok();
            return;
        }

        if self.max_results > 0 && collected.len() == self.max_results {
            collected.remove(0);
        }

        collected.push(message);
    }
    #[allow(dead_code)]
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
    #[allow(dead_code)]
    pub fn process_update(&self, message: SignedMessage) {
        let filters = self.filters.lock();
        for filter in filters.values() {
            filter.collect_message(message.clone());
        }
    }

    pub fn install(&self) -> Result<Arc<MemPoolFilter>, String> {
        let filter = MemPoolFilter::new(self.max_filter_results).map_err(|e| e.to_string())?;
        let id = filter.id.clone();

        let mut filters = self.filters.lock();
        filters.insert(id, filter.clone());

        Ok(filter)
    }

    pub fn remove(&self, id: &FilterID) -> Result<(), String> {
        let mut filters = self.filters.lock();
        filters
            .remove(id)
            .ok_or_else(|| "filter not found".to_string())?;
        Ok(())
    }
}
