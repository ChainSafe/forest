// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! # Ethereum Event Filters Module
//!
//! This module provides the structures and logic necessary to manage filters for Ethereum
//! events, tipsets, and mempool operations. Ethereum event filters enable clients to monitor
//! and subscribe to changes in the blockchain, such as log events, pending transactions in the
//! mempool, or new tipsets in the chain. These filters can be customized to capture specific events
//! or conditions based on various parameters.
//!
//! ## Filter Types:
//!
//! - **Event Filter**: Captures blockchain events, such as smart contract log events, emitted by specific actors.
//! - **TipSet Filter**: Tracks changes in the blockchain's tipset (the latest set of blocks).
//! - **Mempool Filter**: Monitors the Ethereum mempool for new pending transactions that meet certain criteria.
mod event;
mod mempool;
mod store;
mod tipset;

use super::get_tipset_from_hash;
use super::BlockNumberOrHash;
use super::CollectedEvent;
use super::Predefined;
use crate::blocks::Tipset;
use crate::chain::index::ResolveNullTipset;
use crate::cli_shared::cli::EventsConfig;
use crate::rpc::eth::filter::event::*;
use crate::rpc::eth::filter::mempool::*;
use crate::rpc::eth::filter::tipset::*;
use crate::rpc::eth::types::*;
use crate::rpc::eth::EVM_WORD_LENGTH;
use crate::rpc::reflect::Ctx;
use crate::rpc::types::EventEntry;
use crate::shim::address::Address;
use crate::shim::clock::ChainEpoch;
use crate::shim::executor::Entry;
use crate::state_manager::StateEvents;
use crate::utils::misc::env::env_or_default;
use ahash::AHashMap as HashMap;
use anyhow::{anyhow, bail, ensure, Context, Error};
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::IPLD_RAW;
use serde::*;
use std::ops::RangeInclusive;
use std::sync::Arc;
use store::*;

pub trait Matcher {
    fn matches(
        &self,
        eth_emitter_addr: &crate::shim::address::Address,
        entries: &[Entry],
    ) -> anyhow::Result<bool>;
}

/// Trait for managing filters. Provides common functionality for installing and removing filters.
pub trait FilterManager {
    fn install(&self) -> Result<Arc<dyn Filter>, Error>;
    fn remove(&self, filter_id: &FilterID) -> Option<Arc<dyn Filter>>;
}

/// Handles Ethereum event filters, providing an interface for creating and managing filters.
///
/// The `EthEventHandler` structure is the central point for managing Ethereum filters,
/// including event filters and tipSet filters. It interacts with a filter store and manages
/// configurations such as the maximum filter height range and maximum filter results.
pub struct EthEventHandler {
    filter_store: Option<Arc<dyn FilterStore>>,
    pub max_filter_results: usize,
    pub max_filter_height_range: ChainEpoch,
    event_filter_manager: Option<Arc<EventFilterManager>>,
    tipset_filter_manager: Option<Arc<TipSetFilterManager>>,
    mempool_filter_manager: Option<Arc<MempoolFilterManager>>,
}

impl EthEventHandler {
    pub fn new() -> Self {
        let config = EventsConfig::default();
        Self::from_config(&config)
    }

    pub fn from_config(config: &EventsConfig) -> Self {
        let max_filters: usize = env_or_default("FOREST_MAX_FILTERS", 100);
        let max_filter_results = std::env::var("FOREST_MAX_FILTER_RESULTS")
            .ok()
            .and_then(|v| match v.parse::<usize>() {
                Ok(u) if u > 0 => Some(u),
                _ => {
                    tracing::warn!("Invalid FOREST_MAX_FILTER_RESULTS value {v}. A positive integer is expected.");
                    None
                }
            })
            .unwrap_or(config.max_filter_results);
        let max_filter_height_range = std::env::var("FOREST_MAX_FILTER_HEIGHT_RANGE")
            .ok()
            .and_then(|v| match v.parse::<ChainEpoch>() {
                Ok(i) if i > 0 => Some(i),
                _ => {
                    tracing::warn!("Invalid FOREST_MAX_FILTER_HEIGHT_RANGE value {v}. A positive integer is expected.");
                    None
                }
            })
            .unwrap_or(config.max_filter_height_range);
        let filter_store: Option<Arc<dyn FilterStore>> =
            Some(MemFilterStore::new(max_filters) as Arc<dyn FilterStore>);
        let event_filter_manager = Some(EventFilterManager::new(max_filter_results));
        let tipset_filter_manager = Some(TipSetFilterManager::new(max_filter_results));
        let mempool_filter_manager = Some(MempoolFilterManager::new(max_filter_results));

        Self {
            filter_store,
            max_filter_results,
            max_filter_height_range,
            event_filter_manager,
            tipset_filter_manager,
            mempool_filter_manager,
        }
    }

    // Installs an eth filter based on given filter spec.
    pub fn eth_new_filter(
        &self,
        filter_spec: &EthFilterSpec,
        chain_height: i64,
    ) -> Result<FilterID, Error> {
        if let Some(event_filter_manager) = &self.event_filter_manager {
            let pf = filter_spec
                .parse_eth_filter_spec(chain_height, self.max_filter_height_range)
                .context("Parsing error")?;

            let filter = event_filter_manager
                .install(pf)
                .context("Installation error")?;

            if let Some(filter_store) = &self.filter_store {
                if let Err(err) = filter_store.add(filter.clone()) {
                    ensure!(
                        event_filter_manager.remove(filter.id()).is_some(),
                        "Filter not found"
                    );
                    bail!("Adding filter failed: {}", err);
                }
            }
            Ok(filter.id().clone())
        } else {
            Err(Error::msg("NotSupported"))
        }
    }

    fn install_filter(
        &self,
        filter_manager: &Option<Arc<dyn FilterManager>>,
    ) -> Result<FilterID, Error> {
        if let Some(manager) = filter_manager {
            let filter = manager.install().context("Installation error")?;
            if let Some(filter_store) = &self.filter_store {
                if let Err(err) = filter_store.add(filter.clone()) {
                    ensure!(manager.remove(filter.id()).is_some(), "Filter not found");
                    bail!("Adding filter failed: {}", err);
                }
            }
            Ok(filter.id().clone())
        } else {
            Err(Error::msg("NotSupported"))
        }
    }

    // Installs an eth block filter
    pub fn eth_new_block_filter(&self) -> Result<FilterID, Error> {
        let filter_manager: Option<Arc<dyn FilterManager>> = self
            .tipset_filter_manager
            .as_ref()
            .map(|fm| Arc::clone(fm) as Arc<dyn FilterManager>);
        self.install_filter(&filter_manager)
    }

    // Installs an eth pending transaction filter
    pub fn eth_new_pending_transaction_filter(&self) -> Result<FilterID, Error> {
        let filter_manager: Option<Arc<dyn FilterManager>> = self
            .mempool_filter_manager
            .as_ref()
            .map(|fm| Arc::clone(fm) as Arc<dyn FilterManager>);
        self.install_filter(&filter_manager)
    }

    fn uninstall_filter(&self, filter: Arc<dyn Filter>) -> Result<(), Error> {
        let id = filter.id();

        if filter.as_any().is::<EventFilter>() {
            self.event_filter_manager
                .as_ref()
                .context("Event filter manager is missing")?
                .remove(id)
                .context("Failed to remove event filter")?;
        } else if filter.as_any().is::<TipSetFilter>() {
            self.tipset_filter_manager
                .as_ref()
                .context("TipSet filter manager is missing")?
                .remove(id)
                .context("Failed to remove tipset filter")?;
        } else if filter.as_any().is::<MempoolFilter>() {
            self.mempool_filter_manager
                .as_ref()
                .context("Mempool filter manager is missing")?
                .remove(id)
                .context("Failed to remove mempool filter")?;
        }

        self.filter_store
            .as_ref()
            .context("Filter store is missing")?
            .remove(id)
            .context("Failed to remove filter from store")?;

        Ok(())
    }

    pub fn eth_uninstall_filter(&self, id: &FilterID) -> Result<bool, Error> {
        let store = self
            .filter_store
            .as_ref()
            .context("Filter store is not supported")?;

        if let Ok(filter) = store.get(id) {
            self.uninstall_filter(filter)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn parse_eth_filter_spec<DB: Blockstore>(
        &self,
        ctx: &Ctx<DB>,
        filter_spec: &EthFilterSpec,
    ) -> anyhow::Result<ParsedFilter> {
        EthFilterSpec::parse_eth_filter_spec(
            filter_spec,
            ctx.chain_store().heaviest_tipset().epoch(),
            self.max_filter_height_range,
        )
    }

    fn do_match(spec: &EthFilterSpec, eth_emitter_addr: &EthAddress, entries: &[Entry]) -> bool {
        fn get_word(value: &[u8]) -> Option<&[u8; EVM_WORD_LENGTH]> {
            value.get(..EVM_WORD_LENGTH)?.try_into().ok()
        }

        let match_addr = if spec.address.is_empty() {
            true
        } else {
            spec.address.iter().any(|other| other == eth_emitter_addr)
        };
        let match_topics = if let Some(spec) = spec.topics.as_ref() {
            let matched = entries.iter().enumerate().all(|(i, entry)| {
                if let Some(slice) = get_word(entry.value()) {
                    let hash: EthHash = (*slice).into();
                    match spec.0.get(i) {
                        Some(EthHashList::List(vec)) => vec.contains(&hash),
                        Some(EthHashList::Single(Some(h))) => h == &hash,
                        _ => true, /* wildcard */
                    }
                } else {
                    // Drop events with mis-sized topics
                    false
                }
            });
            matched
        } else {
            true
        };
        match_addr && match_topics
    }

    pub async fn collect_events<DB: Blockstore + Send + Sync + 'static>(
        ctx: &Ctx<DB>,
        tipset: &Arc<Tipset>,
        spec: Option<&impl Matcher>,
        collected_events: &mut Vec<CollectedEvent>,
    ) -> anyhow::Result<()> {
        let tipset_key = tipset.key().clone();
        let height = tipset.epoch();

        let messages = ctx.chain_store().messages_for_tipset(tipset)?;

        let StateEvents { events, .. } = ctx.state_manager.tipset_state_events(tipset).await?;

        ensure!(
            messages.len() == events.len(),
            "Length of messages and events do not match"
        );

        let mut event_count = 0;
        for (i, (message, events)) in messages.iter().zip(events.into_iter()).enumerate() {
            for event in events.iter() {
                let id_addr = Address::new_id(event.emitter());
                let result = ctx
                    .state_manager
                    .resolve_to_deterministic_address(id_addr, tipset.clone())
                    .await
                    .with_context(|| {
                        format!(
                            "resolving address {} failed (EPOCH = {})",
                            id_addr,
                            tipset.epoch()
                        )
                    });
                let resolved = if let Ok(resolved) = result {
                    resolved
                } else {
                    // Skip event
                    event_count += 1;
                    continue;
                };

                let entries: Vec<crate::shim::executor::Entry> = event.event().entries();
                // dbg!(&entries);

                let matched = if let Some(spec) = spec {
                    let matched = spec.matches(&resolved, &entries)?;
                    tracing::debug!(
                        "Event {} {}match filter topics",
                        event_count,
                        if matched { "" } else { "do not " }
                    );
                    matched
                } else {
                    true
                };
                if matched {
                    let entries: Vec<EventEntry> = entries
                        .into_iter()
                        .map(|entry| {
                            let (flags, key, codec, value) = entry.into_parts();
                            EventEntry {
                                flags,
                                key,
                                codec,
                                value: value.into(),
                            }
                        })
                        .collect();

                    let ce = CollectedEvent {
                        entries,
                        emitter_addr: resolved,
                        event_idx: event_count,
                        reverted: false,
                        height,
                        tipset_key: tipset_key.clone(),
                        msg_idx: i as u64,
                        msg_cid: message.cid(),
                    };
                    if collected_events.len() >= ctx.eth_event_handler.max_filter_results {
                        bail!("filter matches too many events, try a more restricted filter");
                    }
                    collected_events.push(ce);
                    event_count += 1;
                }
            }
        }

        Ok(())
    }

    pub async fn eth_get_events_for_filter<DB: Blockstore + Send + Sync + 'static>(
        &self,
        ctx: &Ctx<DB>,
        spec: EthFilterSpec,
    ) -> anyhow::Result<Vec<CollectedEvent>> {
        let pf = self.parse_eth_filter_spec(ctx, &spec)?;

        let mut collected_events = vec![];
        match pf.tipsets {
            ParsedFilterTipsets::Hash(block_hash) => {
                let tipset = get_tipset_from_hash(ctx.chain_store(), &block_hash)?;
                let tipset = Arc::new(tipset);
                Self::collect_events(ctx, &tipset, Some(&spec), &mut collected_events).await?;
            }
            ParsedFilterTipsets::Range(range) => {
                let max_height = if *range.end() == -1 {
                    // heaviest tipset doesn't have events because its messages haven't been executed yet
                    ctx.chain_store().heaviest_tipset().epoch() - 1
                } else if *range.end() < 0 {
                    bail!("max_height requested is less than 0")
                } else if *range.end() > ctx.chain_store().heaviest_tipset().epoch() - 1 {
                    // we can't return events for the heaviest tipset as the transactions in that tipset will be executed
                    // in the next non-null tipset (because of Filecoin's "deferred execution" model)
                    bail!("max_height requested is greater than the heaviest tipset");
                } else {
                    *range.end()
                };

                let max_tipset = ctx.chain_store().chain_index.tipset_by_height(
                    max_height,
                    ctx.chain_store().heaviest_tipset(),
                    ResolveNullTipset::TakeOlder,
                )?;
                for tipset in max_tipset
                    .as_ref()
                    .clone()
                    .chain(&ctx.store())
                    .take_while(|ts| ts.epoch() >= *range.start())
                {
                    let tipset = Arc::new(tipset);
                    Self::collect_events(ctx, &tipset, Some(&spec), &mut collected_events).await?;
                }
            }
        }

        Ok(collected_events)
    }
}

impl EthFilterSpec {
    fn parse_eth_filter_spec(
        &self,
        chain_height: i64,
        max_filter_height_range: i64,
    ) -> Result<ParsedFilter, Error> {
        let tipsets = if let Some(block_hash) = &self.block_hash {
            if self.from_block.is_some() || self.to_block.is_some() {
                bail!("must not specify block hash and from/to block");
            }
            ParsedFilterTipsets::Hash(block_hash.clone())
        } else {
            let from_block = self.from_block.as_deref().unwrap_or("");
            let to_block = self.to_block.as_deref().unwrap_or("");
            let (min, max) = parse_block_range(
                chain_height,
                BlockNumberOrHash::from_str(from_block)?,
                BlockNumberOrHash::from_str(to_block)?,
                max_filter_height_range,
            )?;
            ParsedFilterTipsets::Range(RangeInclusive::new(min, max))
        };

        let addresses: Vec<_> = self
            .address
            .iter()
            .map(|ea| {
                ea.to_filecoin_address()
                    .map_err(|e| anyhow!("invalid address {}", e))
            })
            .collect::<Result<Vec<_>, _>>()?;

        let keys = if let Some(topics) = &self.topics {
            keys_to_keys_with_codec(parse_eth_topics(topics)?)
        } else {
            HashMap::new()
        };

        Ok(ParsedFilter {
            tipsets,
            addresses,
            keys,
        })
    }
}

impl Matcher for EthFilterSpec {
    fn matches(
        &self,
        resolved: &crate::shim::address::Address,
        entries: &[Entry],
    ) -> anyhow::Result<bool> {
        fn get_word(value: &[u8]) -> Option<&[u8; EVM_WORD_LENGTH]> {
            value.get(..EVM_WORD_LENGTH)?.try_into().ok()
        }

        let eth_emitter_addr = EthAddress::from_filecoin_address(&resolved)?;

        let match_addr = if self.address.is_empty() {
            true
        } else {
            self.address.iter().any(|other| other == &eth_emitter_addr)
        };
        let match_topics = if let Some(spec) = self.topics.as_ref() {
            let matched = entries.iter().enumerate().all(|(i, entry)| {
                if let Some(slice) = get_word(entry.value()) {
                    let hash: EthHash = (*slice).into();
                    match spec.0.get(i) {
                        Some(EthHashList::List(vec)) => vec.contains(&hash),
                        Some(EthHashList::Single(Some(h))) => h == &hash,
                        _ => true, /* wildcard */
                    }
                } else {
                    // Drop events with mis-sized topics
                    false
                }
            });
            matched
        } else {
            true
        };
        Ok(match_addr && match_topics)
    }
}

fn parse_block_range(
    heaviest: ChainEpoch,
    from_block: BlockNumberOrHash,
    to_block: BlockNumberOrHash,
    max_range: ChainEpoch,
) -> Result<(ChainEpoch, ChainEpoch), Error> {
    let min_height = match from_block {
        BlockNumberOrHash::PredefinedBlock(predefined) => match predefined {
            Predefined::Latest => heaviest,
            Predefined::Earliest => 0,
            _ => heaviest,
        },
        BlockNumberOrHash::BlockNumber(height) => height.into(),
        _ => bail!("Unsupported type for from_block"),
    };

    let max_height = match to_block {
        BlockNumberOrHash::PredefinedBlock(predefined) => match predefined {
            Predefined::Latest => -1,
            Predefined::Earliest => 0,
            _ => -1,
        },
        BlockNumberOrHash::BlockNumber(height) => height.into(),
        _ => bail!("Unsupported type for to_block"),
    };

    if min_height == -1 && max_height > 0 {
        ensure!(
            max_height - heaviest <= max_range,
            "invalid epoch range: to block is too far in the future (maximum: {})",
            max_range
        );
    } else if min_height >= 0 && max_height == -1 {
        ensure!(
            heaviest - min_height <= max_range,
            "invalid epoch range: from block is too far in the past (maximum: {})",
            max_range
        );
    } else if min_height >= 0 && max_height >= 0 {
        ensure!(
            min_height <= max_height,
            "invalid epoch range: to block ({}) must be after from block ({})",
            max_height,
            min_height
        );
        ensure!(
            max_height - min_height <= max_range,
            "invalid epoch range: range between to and from blocks is too large (maximum: {})",
            max_range
        );
    }

    Ok((min_height, max_height))
}

pub fn hex_str_to_epoch(hex_str: &str) -> Result<ChainEpoch, Error> {
    let hex_substring = hex_str
        .strip_prefix("0x")
        .ok_or_else(|| anyhow!("Not a hex"))?;
    i64::from_str_radix(hex_substring, 16)
        .map_err(|e| anyhow!("Failed to convert hex to epoch: {}", e))
}

fn parse_eth_topics(
    EthTopicSpec(topics): &EthTopicSpec,
) -> Result<HashMap<String, Vec<Vec<u8>>>, Error> {
    let mut keys: HashMap<String, Vec<Vec<u8>>> = HashMap::with_capacity(4); // Each eth log entry can contain up to 4 topics

    for (idx, eth_hash_list) in topics.iter().enumerate() {
        let key = format!("t{}", idx + 1);
        match eth_hash_list {
            EthHashList::List(hashes) => {
                let key = format!("t{}", idx + 1);
                for eth_hash in hashes {
                    let EthHash(bytes) = eth_hash;
                    keys.entry(key.clone()).or_default().push(bytes.0.to_vec());
                }
            }
            EthHashList::Single(Some(hash)) => {
                let EthHash(bytes) = hash;
                keys.entry(key.clone()).or_default().push(bytes.0.to_vec());
            }
            EthHashList::Single(None) => {}
        }
    }
    Ok(keys)
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct ActorEventBlock {
    codec: u64,
    value: Vec<u8>,
}

fn keys_to_keys_with_codec(
    keys: HashMap<String, Vec<Vec<u8>>>,
) -> HashMap<String, Vec<ActorEventBlock>> {
    let mut keys_with_codec: HashMap<String, Vec<ActorEventBlock>> =
        HashMap::with_capacity(keys.len());

    keys.into_iter().for_each(|(key, val)| {
        let codec_val: Vec<ActorEventBlock> = val
            .into_iter()
            .map(|v| ActorEventBlock {
                codec: IPLD_RAW,
                value: v,
            })
            .collect();
        keys_with_codec.insert(key, codec_val);
    });

    keys_with_codec
}

#[derive(Debug, PartialEq)]
pub enum ParsedFilterTipsets {
    Range(std::ops::RangeInclusive<ChainEpoch>),
    Hash(EthHash),
}

pub struct ParsedFilter {
    pub(crate) tipsets: ParsedFilterTipsets,
    pub(crate) addresses: Vec<Address>,
    pub(crate) keys: HashMap<String, Vec<ActorEventBlock>>,
}

#[cfg(test)]
mod tests {
    use fvm_shared4::event::Flags;

    use super::*;
    use crate::rpc::eth::{EthAddress, EthFilterSpec, EthTopicSpec};
    use std::str::FromStr;

    #[test]
    fn test_parse_eth_filter_spec() {
        let eth_filter_spec = EthFilterSpec {
            from_block: Some("earliest".into()),
            to_block: Some("latest".into()),
            address: vec![
                EthAddress::from_str("0xff38c072f286e3b20b3954ca9f99c05fbecc64aa").unwrap(),
            ],
            topics: None,
            block_hash: None,
        };

        let chain_height = 50;
        let max_filter_height_range = 100;

        assert!(eth_filter_spec
            .parse_eth_filter_spec(chain_height, max_filter_height_range)
            .is_ok());
    }

    #[test]
    fn test_invalid_parse_eth_filter_spec() {
        let eth_filter_spec = EthFilterSpec {
            from_block: Some("earliest".into()),
            to_block: Some("invalid".into()),
            address: vec![
                EthAddress::from_str("0xff38c072f286e3b20b3954ca9f99c05fbecc64aa").unwrap(),
            ],
            topics: None,
            block_hash: None,
        };

        let chain_height = 50;
        let max_filter_height_range = 100;

        assert!(eth_filter_spec
            .parse_eth_filter_spec(chain_height, max_filter_height_range)
            .is_err(),);
    }

    #[test]
    fn test_parse_block_range() {
        let heaviest = 50;
        let max_range = 100;

        // Test case 1: from_block = "earliest", to_block = "latest"
        let result = parse_block_range(
            heaviest,
            BlockNumberOrHash::from_str("earliest").unwrap(),
            BlockNumberOrHash::from_str("latest").unwrap(),
            max_range,
        );
        assert!(result.is_ok());
        let (min_height, max_height) = result.unwrap();
        assert_eq!(min_height, 0);
        assert_eq!(max_height, -1);

        // Test case 2: from_block = "0x1", to_block = "0xA"
        let result = parse_block_range(
            heaviest,
            BlockNumberOrHash::from_str("0x1").unwrap(),
            BlockNumberOrHash::from_str("0xA").unwrap(),
            max_range,
        );
        assert!(result.is_ok());
        let (min_height, max_height) = result.unwrap();
        assert_eq!(min_height, 1); // hex_str_to_epoch("0x1") = 1
        assert_eq!(max_height, 10); // hex_str_to_epoch("0xA") = 10

        // Test case 3: from_block = "latest", to_block = ""
        let result = parse_block_range(
            heaviest,
            BlockNumberOrHash::from_str("latest").unwrap(),
            BlockNumberOrHash::from_str("").unwrap(),
            max_range,
        );
        assert!(result.is_ok());
        let (min_height, max_height) = result.unwrap();
        assert_eq!(min_height, heaviest);
        assert_eq!(max_height, -1);

        // Test case 4: Range too large
        let result = parse_block_range(
            heaviest,
            BlockNumberOrHash::from_str("earliest").unwrap(),
            BlockNumberOrHash::from_str("0x100").unwrap(),
            max_range,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_keys_to_keys_with_codec() {
        let mut keys: HashMap<String, Vec<Vec<u8>>> = HashMap::new();
        keys.insert("key".to_string(), vec![vec![1, 2, 3]]);

        let result = keys_to_keys_with_codec(keys);

        let res = result.get("key").unwrap();
        assert_eq!(res[0].value, vec![1, 2, 3]);
        assert_eq!(res[0].codec, IPLD_RAW);
    }

    #[test]
    fn test_parse_eth_topics() {
        let topics = EthTopicSpec(vec![EthHashList::List(vec![EthHash::default()])]);
        let actual = parse_eth_topics(&topics).expect("Failed to parse topics");

        let mut expected = HashMap::with_capacity(4);
        expected.insert(
            "t1".to_string(),
            vec![EthHash::default().0.as_bytes().to_vec()],
        );

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_hex_str_to_epoch() {
        // Valid hex string
        let hex_str = "0x0";
        let result = hex_str_to_epoch(hex_str);
        assert_eq!(result.unwrap(), 0);

        // Invalid hex string should fail
        let hex_str = "1a";
        let result = hex_str_to_epoch(hex_str);
        assert!(result.is_err());

        // Invalid hex string should fail
        let hex_str = "0xG";
        let result = hex_str_to_epoch(hex_str);
        assert!(result.is_err());
    }

    #[test]
    fn test_eth_new_filter() {
        let eth_event_handler = EthEventHandler::new();

        let filter_spec = EthFilterSpec {
            from_block: Some("earliest".into()),
            to_block: Some("latest".into()),
            address: vec![
                EthAddress::from_str("0xff38c072f286e3b20b3954ca9f99c05fbecc64aa").unwrap(),
            ],
            topics: None,
            block_hash: None,
        };

        let chain_height = 50;
        let result = eth_event_handler.eth_new_filter(&filter_spec, chain_height);

        assert!(result.is_ok(), "Expected successful filter creation");
    }

    #[test]
    fn test_eth_new_block_filter() {
        let eth_event_handler = EthEventHandler::new();
        let result = eth_event_handler.eth_new_block_filter();

        assert!(result.is_ok(), "Expected successful block filter creation");
    }

    #[test]
    fn test_eth_new_pending_transaction_filter() {
        let eth_event_handler = EthEventHandler::new();
        let result = eth_event_handler.eth_new_pending_transaction_filter();

        assert!(
            result.is_ok(),
            "Expected successful pending transaction filter creation"
        );
    }

    #[test]
    fn test_eth_uninstall_filter() {
        let event_handler = EthEventHandler::new();
        let mut filter_ids = Vec::new();
        let filter_spec = EthFilterSpec {
            from_block: None,
            to_block: None,
            address: vec![
                EthAddress::from_str("0xff38c072f286e3b20b3954ca9f99c05fbecc64aa").unwrap(),
            ],
            topics: None,
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
            assert!(
                result,
                "Uninstalling filter with id {:?} failed",
                &filter_id
            );
        }
    }

    #[test]
    fn test_do_match_address() {
        let empty_spec = EthFilterSpec {
            from_block: None,
            to_block: None,
            address: vec![],
            topics: None,
            block_hash: None,
        };

        let eth_addr0 = EthAddress::from_str("0xff38c072f286e3b20b3954ca9f99c05fbecc64aa").unwrap();

        let eth_addr1 = EthAddress::from_str("0x26937d59db4463254c930d5f31353f14aa89a0f7").unwrap();

        let entries0 = vec![
            Entry::new(
                Flags::FLAG_INDEXED_ALL,
                "t1".into(),
                IPLD_RAW,
                vec![
                    226, 71, 32, 244, 92, 183, 79, 45, 85, 241, 222, 235, 182, 9, 143, 80, 241, 11,
                    81, 29, 171, 138, 125, 71, 196, 129, 154, 8, 220, 208, 184, 149,
                ],
            ),
            Entry::new(
                Flags::FLAG_INDEXED_ALL,
                "t2".into(),
                IPLD_RAW,
                vec![
                    116, 4, 227, 209, 4, 234, 120, 65, 195, 217, 230, 253, 32, 173, 254, 153, 180,
                    173, 88, 107, 192, 141, 143, 59, 211, 175, 239, 137, 76, 241, 132, 222,
                ],
            ),
            Entry::new(
                Flags::FLAG_INDEXED_ALL,
                "d".into(),
                IPLD_RAW,
                vec![
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 23,
                    254, 169, 229, 74, 6, 24, 52, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                    0, 0, 0, 0, 0, 0, 13, 232, 134, 151, 206, 121, 139, 231, 226, 192,
                ],
            ),
        ];

        // Matching an empty spec
        assert!(EthEventHandler::do_match(&empty_spec, &eth_addr0, &[]));

        assert!(EthEventHandler::do_match(
            &empty_spec,
            &eth_addr0,
            &entries0
        ));

        // Matching the given address 0
        let spec0 = EthFilterSpec {
            from_block: None,
            to_block: None,
            address: vec![eth_addr0.clone()],
            topics: None,
            block_hash: None,
        };

        assert!(EthEventHandler::do_match(&spec0, &eth_addr0, &[]));

        assert!(!EthEventHandler::do_match(&spec0, &eth_addr1, &[]));

        // Matching the given address 0 or 1
        let spec1 = EthFilterSpec {
            from_block: None,
            to_block: None,
            address: vec![eth_addr0.clone(), eth_addr1.clone()],
            topics: None,
            block_hash: None,
        };

        assert!(EthEventHandler::do_match(&spec1, &eth_addr0, &[]));

        assert!(EthEventHandler::do_match(&spec1, &eth_addr1, &[]));
    }

    #[test]
    fn test_do_match_topic() {
        let eth_addr0 = EthAddress::from_str("0xff38c072f286e3b20b3954ca9f99c05fbecc64aa").unwrap();

        let entries0 = vec![
            Entry::new(
                Flags::FLAG_INDEXED_ALL,
                "t1".into(),
                IPLD_RAW,
                vec![
                    226, 71, 32, 244, 92, 183, 79, 45, 85, 241, 222, 235, 182, 9, 143, 80, 241, 11,
                    81, 29, 171, 138, 125, 71, 196, 129, 154, 8, 220, 208, 184, 149,
                ],
            ),
            Entry::new(
                Flags::FLAG_INDEXED_ALL,
                "t2".into(),
                IPLD_RAW,
                vec![
                    116, 4, 227, 209, 4, 234, 120, 65, 195, 217, 230, 253, 32, 173, 254, 153, 180,
                    173, 88, 107, 192, 141, 143, 59, 211, 175, 239, 137, 76, 241, 132, 222,
                ],
            ),
            Entry::new(
                Flags::FLAG_INDEXED_ALL,
                "d".into(),
                IPLD_RAW,
                vec![
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 23,
                    254, 169, 229, 74, 6, 24, 52, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                    0, 0, 0, 0, 0, 0, 13, 232, 134, 151, 206, 121, 139, 231, 226, 192,
                ],
            ),
        ];

        let topic0 =
            EthHash::from_str("0xe24720f45cb74f2d55f1deebb6098f50f10b511dab8a7d47c4819a08dcd0b895")
                .unwrap();

        let topic1 =
            EthHash::from_str("0x7404e3d104ea7841c3d9e6fd20adfe99b4ad586bc08d8f3bd3afef894cf184de")
                .unwrap();

        let topic2 =
            EthHash::from_str("0x000000000000000000000000d0fb381fc644cdd5d694d35e1afb445527b9244b")
                .unwrap();

        let topic3 =
            EthHash::from_str("0x00000000000000000000000092c3b379c217fdf8603884770e83fded7b7410f8")
                .unwrap();

        let spec1 = EthFilterSpec {
            from_block: None,
            to_block: None,
            address: vec![],
            topics: Some(EthTopicSpec(vec![EthHashList::Single(None)])),
            block_hash: None,
        };

        assert!(EthEventHandler::do_match(&spec1, &eth_addr0, &entries0));

        let spec2 = EthFilterSpec {
            from_block: None,
            to_block: None,
            address: vec![],
            topics: Some(EthTopicSpec(vec![
                EthHashList::Single(None),
                EthHashList::Single(None),
            ])),
            block_hash: None,
        };

        assert!(EthEventHandler::do_match(&spec2, &eth_addr0, &entries0));

        let spec2 = EthFilterSpec {
            from_block: None,
            to_block: None,
            address: vec![],
            topics: Some(EthTopicSpec(vec![EthHashList::Single(Some(
                topic0.clone(),
            ))])),
            block_hash: None,
        };

        assert!(EthEventHandler::do_match(&spec2, &eth_addr0, &entries0));

        let spec3 = EthFilterSpec {
            from_block: None,
            to_block: None,
            address: vec![],
            topics: Some(EthTopicSpec(vec![EthHashList::List(vec![topic0.clone()])])),
            block_hash: None,
        };

        assert!(EthEventHandler::do_match(&spec3, &eth_addr0, &entries0));

        let spec4 = EthFilterSpec {
            from_block: None,
            to_block: None,
            address: vec![],
            topics: Some(EthTopicSpec(vec![EthHashList::List(vec![
                topic1.clone(),
                topic0.clone(),
            ])])),
            block_hash: None,
        };

        assert!(EthEventHandler::do_match(&spec4, &eth_addr0, &entries0));

        let spec5 = EthFilterSpec {
            from_block: None,
            to_block: None,
            address: vec![],
            topics: Some(EthTopicSpec(vec![EthHashList::Single(Some(
                topic1.clone(),
            ))])),
            block_hash: None,
        };

        assert!(!EthEventHandler::do_match(&spec5, &eth_addr0, &entries0));

        let spec6 = EthFilterSpec {
            from_block: None,
            to_block: None,
            address: vec![],
            topics: Some(EthTopicSpec(vec![EthHashList::List(vec![
                topic2.clone(),
                topic3.clone(),
            ])])),
            block_hash: None,
        };

        assert!(!EthEventHandler::do_match(&spec6, &eth_addr0, &entries0));

        let spec7 = EthFilterSpec {
            from_block: None,
            to_block: None,
            address: vec![],
            topics: Some(EthTopicSpec(vec![
                EthHashList::Single(Some(topic1.clone())),
                EthHashList::Single(Some(topic1.clone())),
            ])),
            block_hash: None,
        };

        assert!(!EthEventHandler::do_match(&spec7, &eth_addr0, &entries0));

        let spec8 = EthFilterSpec {
            from_block: None,
            to_block: None,
            address: vec![],
            topics: Some(EthTopicSpec(vec![
                EthHashList::Single(Some(topic0.clone())),
                EthHashList::Single(Some(topic1.clone())),
                EthHashList::Single(Some(topic3.clone())),
            ])),
            block_hash: None,
        };

        assert!(!EthEventHandler::do_match(&spec8, &eth_addr0, &entries0));
    }
}
