// Copyright 2019-2026 ChainSafe Systems
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
pub mod event;
pub mod mempool;
mod store;
pub mod tipset;

use super::BlockNumberOrHash;
use super::CollectedEvent;
use super::Predefined;
use super::get_tipset_from_hash;
use crate::blocks::Tipset;
use crate::blocks::TipsetKey;
use crate::chain::index::ResolveNullTipset;
use crate::cli_shared::cli::EventsConfig;
use crate::rpc::eth::EVM_WORD_LENGTH;
use crate::rpc::eth::filter::event::*;
use crate::rpc::eth::filter::mempool::*;
use crate::rpc::eth::filter::tipset::*;
use crate::rpc::eth::types::*;
use crate::rpc::misc::ActorEventFilter;
use crate::rpc::reflect::Ctx;
use crate::rpc::types::{Event, EventEntry};
use crate::shim::address::Address;
use crate::shim::clock::ChainEpoch;
use crate::shim::executor::{Entry, StampedEvent};
use crate::state_manager::StateEvents;
use crate::utils::misc::env::env_or_default;
use ahash::AHashMap as HashMap;
use anyhow::{Context, Error, anyhow, bail, ensure};
use cid::Cid;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::IPLD_RAW;
use serde::*;
use std::ops::RangeInclusive;
use std::sync::Arc;
use store::*;

/// A trait for filtering events based on predefined conditions.
///
/// Implementors of this trait define custom logic to determine whether an event matches the filtering criteria
/// based on the event emitter's address and its associated entries.
///
pub trait Matcher {
    /// # Parameters
    /// - `emitter_addr`: The address of the Actor that emitted the event, along with the associated event entries.
    /// - `entries`: A list of [`Entry`] objects related to the event.
    ///
    /// # Returns
    /// - `Ok(true)`: If the event matches the filtering criteria.
    /// - `Ok(false)`: If the event does not match the filtering criteria.
    /// - `Err(anyhow::Error)`: If an error occurs during the evaluation of the filtering logic.
    ///
    /// # Notes
    /// - Implementations may use wildcards to match any emitter address or topic.
    fn matches(
        &self,
        emitter_addr: &crate::shim::address::Address,
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
    pub filter_store: Option<Arc<dyn FilterStore>>,
    pub max_filter_results: usize,
    pub max_filter_height_range: ChainEpoch,
    event_filter_manager: Option<Arc<EventFilterManager>>,
    tipset_filter_manager: Option<Arc<TipSetFilterManager>>,
    mempool_filter_manager: Option<Arc<MempoolFilterManager>>,
}

#[derive(Clone, Copy)]
pub enum SkipEvent {
    OnUnresolvedAddress,
    Never,
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

            if let Some(filter_store) = &self.filter_store
                && let Err(err) = filter_store.add(filter.clone())
            {
                ensure!(
                    event_filter_manager.remove(filter.id()).is_some(),
                    "Filter not found"
                );
                bail!("Adding filter failed: {}", err);
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
            if let Some(filter_store) = &self.filter_store
                && let Err(err) = filter_store.add(filter.clone())
            {
                ensure!(manager.remove(filter.id()).is_some(), "Filter not found");
                bail!("Adding filter failed: {}", err);
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

    pub fn parse_eth_filter_spec<DB: Blockstore>(
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

    pub async fn collect_events<DB: Blockstore + Send + Sync + 'static>(
        ctx: &Ctx<DB>,
        tipset: &Tipset,
        spec: Option<&impl Matcher>,
        skip_event: SkipEvent,
        collected_events: &mut Vec<CollectedEvent>,
    ) -> anyhow::Result<()> {
        let tipset_key = tipset.key().clone();
        let height = tipset.epoch();

        let messages = ctx.chain_store().messages_for_tipset(tipset)?;

        let StateEvents { events, .. } = ctx.state_manager.tipset_state_events(tipset).await?;

        ensure!(
            messages.len() == events.len(),
            "Length of messages ({}) and events ({}) do not match",
            messages.len(),
            events.len(),
        );

        let mut event_count = 0;
        for (i, (message, events)) in messages.iter().zip(events.into_iter()).enumerate() {
            for event in events.iter() {
                let id_addr = Address::new_id(event.emitter());
                let result = ctx
                    .state_manager
                    .resolve_to_deterministic_address(id_addr, tipset)
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
                    event_count += 1;
                    if let SkipEvent::OnUnresolvedAddress = skip_event {
                        // Skip event
                        continue;
                    } else {
                        id_addr
                    }
                };

                let entries: Vec<crate::shim::executor::Entry> = event.event().entries();

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

    /// Gets events by event root.
    pub fn get_events_by_event_root<DB: Blockstore + Send + Sync + 'static>(
        ctx: &Ctx<DB>,
        events_root: &Cid,
    ) -> anyhow::Result<Vec<Event>> {
        let state_events =
            match StampedEvent::get_events(ctx.chain_store().blockstore(), events_root) {
                Ok(e) => e,
                Err(e) => {
                    return Err(anyhow::anyhow!("load events amt: {}", e));
                }
            };

        let chain_events: Vec<Event> = state_events.into_iter().map(Into::into).collect();
        Ok(chain_events)
    }

    pub async fn get_events_for_parsed_filter<DB: Blockstore + Send + Sync + 'static>(
        &self,
        ctx: &Ctx<DB>,
        pf: &ParsedFilter,
        skip_event: SkipEvent,
    ) -> anyhow::Result<Vec<CollectedEvent>> {
        let mut collected_events = vec![];
        match &pf.tipsets {
            ParsedFilterTipsets::Hash(block_hash) => {
                let tipset = get_tipset_from_hash(ctx.chain_store(), block_hash)?;
                let tipset = Arc::new(tipset);
                Self::collect_events(ctx, &tipset, Some(pf), skip_event, &mut collected_events)
                    .await?;
            }
            ParsedFilterTipsets::Key(tsk) => {
                let tipset = Arc::new(Tipset::load_required(ctx.store(), tsk)?);
                Self::collect_events(ctx, &tipset, Some(pf), skip_event, &mut collected_events)
                    .await?;
            }
            ParsedFilterTipsets::Range(range) => {
                // we can't return events for the heaviest tipset as the transactions in that tipset will be executed
                // in the next non-null tipset (because of Filecoin's "deferred execution" model)
                let heaviest_epoch = ctx.chain_store().heaviest_tipset().epoch();
                ensure!(
                    *range.end() < heaviest_epoch,
                    "max_height requested is greater than the heaviest tipset"
                );
                let max_height = if *range.end() == -1 {
                    // heaviest tipset doesn't have events because its messages haven't been executed yet
                    heaviest_epoch - 1
                } else {
                    *range.end()
                };

                let max_tipset = ctx.chain_index().tipset_by_height(
                    max_height,
                    ctx.chain_store().heaviest_tipset(),
                    ResolveNullTipset::TakeOlder,
                )?;
                for tipset in max_tipset
                    .chain(&ctx.store())
                    .take_while(|ts| ts.epoch() >= *range.start())
                {
                    Self::collect_events(ctx, &tipset, Some(pf), skip_event, &mut collected_events)
                        .await?;
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
            ParsedFilterTipsets::Hash(*block_hash)
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

        let addresses: Vec<_> = if let Some(ref address_list) = self.address {
            address_list
                .iter()
                .map(|ea| {
                    ea.to_filecoin_address()
                        .map_err(|e| anyhow!("invalid address {}", e))
                })
                .collect::<Result<Vec<_>, _>>()?
        } else {
            vec![]
        };

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
        emitter_addr: &crate::shim::address::Address,
        entries: &[Entry],
    ) -> anyhow::Result<bool> {
        fn get_word(value: &[u8]) -> Option<&[u8; EVM_WORD_LENGTH]> {
            value.get(..EVM_WORD_LENGTH)?.try_into().ok()
        }

        let eth_emitter_addr = EthAddress::from_filecoin_address(emitter_addr)?;

        let match_addr = match self.address {
            Some(ref address_list) => {
                if address_list.is_empty() {
                    true
                } else {
                    address_list.iter().any(|other| other == &eth_emitter_addr)
                }
            }
            None => true,
        };

        let match_topics = if let Some(spec) = self.topics.as_ref() {
            entries.iter().enumerate().all(|(i, entry)| {
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
            })
        } else {
            true
        };
        Ok(match_addr && match_topics)
    }
}

// TODO(forest): https://github.com/ChainSafe/forest/issues/6411
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

    ensure!(
        max_height >= 0 || max_height == -1,
        "max_height requested is less than 0"
    );

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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
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

#[derive(Clone, Debug, PartialEq)]
pub enum ParsedFilterTipsets {
    Range(RangeInclusive<ChainEpoch>),
    Hash(EthHash),
    Key(TipsetKey),
}

#[derive(Debug)]
pub struct ParsedFilter {
    pub(crate) tipsets: ParsedFilterTipsets,
    pub(crate) addresses: Vec<Address>,
    pub(crate) keys: HashMap<String, Vec<ActorEventBlock>>,
}

impl ParsedFilter {
    pub fn new_with_tipset(tipsets: ParsedFilterTipsets) -> Self {
        ParsedFilter {
            tipsets,
            addresses: vec![],
            keys: HashMap::new(),
        }
    }
    pub fn from_actor_event_filter(
        chain_height: ChainEpoch,
        _max_filter_height_range: ChainEpoch,
        filter: ActorEventFilter,
    ) -> anyhow::Result<Self> {
        let tipsets = if let Some(tsk) = &filter.tipset_key {
            if filter.from_height.is_some() || filter.to_height.is_some() {
                bail!("must not specify block hash and from/to block");
            }
            ParsedFilterTipsets::Key(tsk.0.clone())
        } else {
            let min = filter.from_height.unwrap_or(0);
            let max = filter.to_height.unwrap_or(chain_height);
            ParsedFilterTipsets::Range(RangeInclusive::new(min, max))
        };

        let addresses: Vec<_> = filter.addresses.iter().map(|addr| addr.0).collect();

        let mut keys: HashMap<String, Vec<ActorEventBlock>> = Default::default();
        for (k, v) in filter.fields.into_iter() {
            let data = v
                .into_iter()
                .map(|x| crate::rpc::methods::eth::filter::ActorEventBlock {
                    codec: x.codec,
                    value: x.value.0,
                })
                .collect();
            keys.insert(k, data);
        }

        Ok(ParsedFilter {
            tipsets,
            addresses,
            keys,
        })
    }
}

impl Matcher for ParsedFilter {
    fn matches(
        &self,
        emitter_addr: &crate::shim::address::Address,
        entries: &[Entry],
    ) -> anyhow::Result<bool> {
        let match_addr = if self.addresses.is_empty() {
            true
        } else {
            self.addresses.contains(emitter_addr)
        };

        let match_fields = if self.keys.is_empty() {
            true
        } else {
            self.keys.iter().all(|(k, v)| {
                entries.iter().any(|entry| {
                    k == entry.key()
                        && v.iter()
                            .any(|aeb| aeb.codec == entry.codec() && &aeb.value == entry.value())
                })
            })
        };

        Ok(match_addr && match_fields)
    }
}

impl Matcher for EventFilter {
    fn matches(
        &self,
        resolved: &crate::shim::address::Address,
        _entries: &[Entry],
    ) -> anyhow::Result<bool> {
        let match_addr = if self.addresses.is_empty() {
            true
        } else {
            self.addresses.contains(resolved)
        };
        Ok(match_addr)
    }
}

#[cfg(test)]
mod tests {
    use ahash::AHashMap;
    use base64::{Engine, prelude::BASE64_STANDARD};
    use fvm_ipld_encoding::DAG_CBOR;
    use fvm_shared4::event::Flags;

    use super::*;
    use crate::rpc::eth::{EthAddress, EthFilterSpec, EthTopicSpec};
    use std::str::FromStr;

    #[test]
    fn test_parse_eth_filter_spec() {
        let eth_filter_spec = EthFilterSpec {
            from_block: Some("earliest".into()),
            to_block: Some("latest".into()),
            address: Some(
                vec![EthAddress::from_str("0xff38c072f286e3b20b3954ca9f99c05fbecc64aa").unwrap()]
                    .into(),
            ),
            ..Default::default()
        };

        let chain_height = 50;
        let max_filter_height_range = 100;

        assert!(
            eth_filter_spec
                .parse_eth_filter_spec(chain_height, max_filter_height_range)
                .is_ok()
        );
    }

    #[test]
    fn test_empty_address_list() {
        let empty_list_spec = EthFilterSpec {
            address: Some(vec![].into()), // Empty list, not None
            ..Default::default()
        };

        let addr = Address::from_str("t410f744ma4xsq3r3eczzktfj7goal67myzfkusna2hy").unwrap();

        // Updated to match Lotus behavior: empty list = wildcard (matches all)
        assert!(empty_list_spec.matches(&addr, &[]).unwrap());
    }

    #[test]
    fn test_parse_eth_filter_spec_with_none_address() {
        let eth_filter_spec = EthFilterSpec {
            from_block: Some("latest".into()),
            to_block: Some("latest".into()),
            address: None,
            ..Default::default()
        };

        let chain_height = 50;
        let max_filter_height_range = 100;

        let result = eth_filter_spec.parse_eth_filter_spec(chain_height, max_filter_height_range);

        assert!(result.is_ok());
        let parsed = result.unwrap();
        assert!(parsed.addresses.is_empty());
    }

    #[test]
    fn test_eth_new_filter_with_none_address() {
        let eth_event_handler = EthEventHandler::new();

        let filter_spec = EthFilterSpec {
            from_block: Some("latest".into()),
            to_block: Some("latest".into()),
            address: None,
            ..Default::default()
        };

        let chain_height = 50;
        let result = eth_event_handler.eth_new_filter(&filter_spec, chain_height);

        assert!(
            result.is_ok(),
            "Expected successful filter creation with None address"
        );
    }

    #[test]
    fn test_lotus_compatible_address_behavior() {
        // Test the Lotus-compatible behavior: empty list = wildcard
        let addr = Address::from_str("t410f744ma4xsq3r3eczzktfj7goal67myzfkusna2hy").unwrap();

        // Case 1: None (omitted) = wildcard
        let none_spec = EthFilterSpec {
            address: None,
            ..Default::default()
        };
        assert!(
            none_spec.matches(&addr, &[]).unwrap(),
            "None should match all addresses"
        );

        // Case 2: Empty list = wildcard (Lotus behavior)
        let empty_spec = EthFilterSpec {
            address: Some(vec![].into()),
            ..Default::default()
        };
        assert!(
            empty_spec.matches(&addr, &[]).unwrap(),
            "Empty list should match all addresses (Lotus compatible)"
        );

        // Case 3: Specific address = only that address
        let eth_addr = EthAddress::from_filecoin_address(&addr).unwrap();
        let specific_spec = EthFilterSpec {
            address: Some(vec![eth_addr].into()),
            ..Default::default()
        };
        assert!(
            specific_spec.matches(&addr, &[]).unwrap(),
            "Specific address should match itself"
        );

        // Case 4: Different address = no match
        let different_addr =
            Address::from_str("t410fe2jx2wo3irrsktetbvptcnj7csvitihxyehuaeq").unwrap();
        assert!(
            !specific_spec.matches(&different_addr, &[]).unwrap(),
            "Specific address should not match different address"
        );
    }

    #[test]
    fn test_eth_filter_spec_default_has_none_values() {
        let default_spec = EthFilterSpec::default();

        // Check all fields have their expected default values
        assert!(
            default_spec.from_block.is_none(),
            "Default EthFilterSpec should have None from_block"
        );
        assert!(
            default_spec.to_block.is_none(),
            "Default EthFilterSpec should have None to_block"
        );
        assert!(
            default_spec.address.is_none(),
            "Default EthFilterSpec should have None address"
        );
        assert!(
            default_spec.topics.is_none(),
            "Default EthFilterSpec should have None topics"
        );
        assert!(
            default_spec.block_hash.is_none(),
            "Default EthFilterSpec should have None block_hash"
        );

        // Verify that the default spec matches any address (wildcard behavior)
        let addr0 = Address::from_str("t410f744ma4xsq3r3eczzktfj7goal67myzfkusna2hy").unwrap();
        let addr1 = Address::from_str("t410fe2jx2wo3irrsktetbvptcnj7csvitihxyehuaeq").unwrap();

        // Test with no entries
        assert!(default_spec.matches(&addr0, &[]).unwrap());
        assert!(default_spec.matches(&addr1, &[]).unwrap());
    }

    #[test]
    fn test_invalid_parse_eth_filter_spec() {
        let eth_filter_spec = EthFilterSpec {
            from_block: Some("earliest".into()),
            to_block: Some("invalid".into()),
            address: Some(
                vec![EthAddress::from_str("0xff38c072f286e3b20b3954ca9f99c05fbecc64aa").unwrap()]
                    .into(),
            ),
            ..Default::default()
        };

        let chain_height = 50;
        let max_filter_height_range = 100;

        assert!(
            eth_filter_spec
                .parse_eth_filter_spec(chain_height, max_filter_height_range)
                .is_err(),
        );
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

        // Test case 5: from_block = "latest", to_block = "earliest"
        let result = parse_block_range(
            heaviest,
            BlockNumberOrHash::from_str("latest").unwrap(),
            BlockNumberOrHash::from_str("earliest").unwrap(),
            max_range,
        );
        assert!(result.is_err());

        // Test case 6: from_block = "earliest", to_block = "earliest"
        let result = parse_block_range(
            heaviest,
            BlockNumberOrHash::from_str("earliest").unwrap(),
            BlockNumberOrHash::from_str("earliest").unwrap(),
            max_range,
        );
        assert!(result.is_ok());
        let (min_height, max_height) = result.unwrap();
        assert_eq!(min_height, 0);
        assert_eq!(max_height, 0);

        // Test case 7: from_block = "latest", to_block = "latest"
        let result = parse_block_range(
            heaviest,
            BlockNumberOrHash::from_str("latest").unwrap(),
            BlockNumberOrHash::from_str("latest").unwrap(),
            max_range,
        );
        assert!(result.is_ok());
        let (min_height, max_height) = result.unwrap();
        assert_eq!(min_height, heaviest);
        assert_eq!(max_height, -1);

        // Test case 8: from_block = "earliest", to_block = ""
        let result = parse_block_range(
            heaviest,
            BlockNumberOrHash::from_str("earliest").unwrap(),
            BlockNumberOrHash::from_str("").unwrap(),
            max_range,
        );
        assert!(result.is_ok());
        let (min_height, max_height) = result.unwrap();
        assert_eq!(min_height, 0);
        assert_eq!(max_height, -1);

        // Test case 9: from_block = "", to_block = "earliest"
        let result = parse_block_range(
            heaviest,
            BlockNumberOrHash::from_str("").unwrap(),
            BlockNumberOrHash::from_str("earliest").unwrap(),
            max_range,
        );
        assert!(result.is_err());

        // Test case 10: from_block = "", to_block = "latest"
        let result = parse_block_range(
            heaviest,
            BlockNumberOrHash::from_str("").unwrap(),
            BlockNumberOrHash::from_str("latest").unwrap(),
            max_range,
        );
        assert!(result.is_ok());
        let (min_height, max_height) = result.unwrap();
        assert_eq!(min_height, heaviest);
        assert_eq!(max_height, -1);

        // Test case 11: from_block = "", to_block = ""
        let result = parse_block_range(
            heaviest,
            BlockNumberOrHash::from_str("").unwrap(),
            BlockNumberOrHash::from_str("").unwrap(),
            max_range,
        );
        assert!(result.is_ok());
        let (min_height, max_height) = result.unwrap();
        assert_eq!(min_height, heaviest);
        assert_eq!(max_height, -1);

        // Test case 12: Both blocks are non-negative but from_block > to_block.
        let result = parse_block_range(
            heaviest,
            BlockNumberOrHash::from_str("0xA").unwrap(),
            BlockNumberOrHash::from_str("0x1").unwrap(),
            max_range,
        );
        assert!(result.is_err());

        // Test case 13: Both blocks are non-negative, order is correct, but the range is too large.
        let result = parse_block_range(
            heaviest,
            BlockNumberOrHash::from_str("earliest").unwrap(),
            BlockNumberOrHash::from_str("0x65").unwrap(),
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
            address: Some(
                vec![EthAddress::from_str("0xff38c072f286e3b20b3954ca9f99c05fbecc64aa").unwrap()]
                    .into(),
            ),
            ..Default::default()
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
            address: Some(
                vec![EthAddress::from_str("0xff38c072f286e3b20b3954ca9f99c05fbecc64aa").unwrap()]
                    .into(),
            ),
            ..Default::default()
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
        let empty_spec = EthFilterSpec::default();

        let addr0 = Address::from_str("t410f744ma4xsq3r3eczzktfj7goal67myzfkusna2hy").unwrap();
        let eth_addr0 = EthAddress::from_str("0xff38c072f286e3b20b3954ca9f99c05fbecc64aa").unwrap();

        let addr1 = Address::from_str("t410fe2jx2wo3irrsktetbvptcnj7csvitihxyehuaeq").unwrap();
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
        assert!(empty_spec.matches(&addr0, &[]).unwrap());

        assert!(empty_spec.matches(&addr0, &entries0).unwrap());

        // Matching the given address 0
        let spec0 = EthFilterSpec {
            address: Some(vec![eth_addr0].into()),
            ..Default::default()
        };

        assert!(spec0.matches(&addr0, &[]).unwrap());

        assert!(!spec0.matches(&addr1, &[]).unwrap());

        // Matching the given address 0 or 1
        let spec1 = EthFilterSpec {
            address: Some(vec![eth_addr0, eth_addr1].into()),
            ..Default::default()
        };

        assert!(spec1.matches(&addr0, &[]).unwrap());

        assert!(spec1.matches(&addr1, &[]).unwrap());
    }

    #[test]
    fn test_do_match_topic() {
        let addr0 = Address::from_str("t410f744ma4xsq3r3eczzktfj7goal67myzfkusna2hy").unwrap();

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
            topics: Some(EthTopicSpec(vec![EthHashList::Single(None)])),
            ..Default::default()
        };

        assert!(spec1.matches(&addr0, &entries0).unwrap());

        let spec2 = EthFilterSpec {
            topics: Some(EthTopicSpec(vec![
                EthHashList::Single(None),
                EthHashList::Single(None),
            ])),
            ..Default::default()
        };

        assert!(spec2.matches(&addr0, &entries0).unwrap());

        let spec2 = EthFilterSpec {
            topics: Some(EthTopicSpec(vec![EthHashList::Single(Some(topic0))])),
            ..Default::default()
        };

        assert!(spec2.matches(&addr0, &entries0).unwrap());

        let spec3 = EthFilterSpec {
            topics: Some(EthTopicSpec(vec![EthHashList::List(vec![topic0])])),
            ..Default::default()
        };

        assert!(spec3.matches(&addr0, &entries0).unwrap());

        let spec4 = EthFilterSpec {
            topics: Some(EthTopicSpec(vec![EthHashList::List(vec![topic1, topic0])])),
            ..Default::default()
        };

        assert!(spec4.matches(&addr0, &entries0).unwrap());

        let spec5 = EthFilterSpec {
            topics: Some(EthTopicSpec(vec![EthHashList::Single(Some(topic1))])),
            ..Default::default()
        };

        assert!(!spec5.matches(&addr0, &entries0).unwrap());

        let spec6 = EthFilterSpec {
            topics: Some(EthTopicSpec(vec![EthHashList::List(vec![topic2, topic3])])),
            ..Default::default()
        };

        assert!(!spec6.matches(&addr0, &entries0).unwrap());

        let spec7 = EthFilterSpec {
            topics: Some(EthTopicSpec(vec![
                EthHashList::Single(Some(topic1)),
                EthHashList::Single(Some(topic1)),
            ])),
            ..Default::default()
        };

        assert!(!spec7.matches(&addr0, &entries0).unwrap());

        let spec8 = EthFilterSpec {
            topics: Some(EthTopicSpec(vec![
                EthHashList::Single(Some(topic0)),
                EthHashList::Single(Some(topic1)),
                EthHashList::Single(Some(topic3)),
            ])),
            ..Default::default()
        };

        assert!(!spec8.matches(&addr0, &entries0).unwrap());
    }

    #[test]
    fn test_parsed_filter_match_address() {
        // Note that all the following addresses and topics (base64-encoded strings) come from real data on Calibnet,
        // but they could also be newly generated addresses or valid topic data.

        let empty_filter = ParsedFilter {
            tipsets: ParsedFilterTipsets::Range(0..=0),
            addresses: vec![],
            keys: Default::default(),
        };

        let addr0 = Address::from_str("t410f744ma4xsq3r3eczzktfj7goal67myzfkusna2hy").unwrap();

        let addr1 = Address::from_str("t410fe2jx2wo3irrsktetbvptcnj7csvitihxyehuaeq").unwrap();

        let entries0 = vec![
            Entry::new(
                Flags::FLAG_INDEXED_ALL,
                "t1".into(),
                IPLD_RAW,
                BASE64_STANDARD
                    .decode("4kcg9Fy3Ty1V8d7rtgmPUPELUR2rin1HxIGaCNzQuJU=")
                    .unwrap(),
            ),
            Entry::new(
                Flags::FLAG_INDEXED_ALL,
                "t2".into(),
                IPLD_RAW,
                BASE64_STANDARD
                    .decode("dATj0QTqeEHD2eb9IK3+mbStWGvAjY8706/viUzxhN4=")
                    .unwrap(),
            ),
            Entry::new(
                Flags::FLAG_INDEXED_ALL,
                "d".into(),
                IPLD_RAW,
                BASE64_STANDARD
                    .decode("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAGCFA6vK+FJsAAAAAAAAAAAAAAAAAAAAAAAAAAAAAFLvXoMoks6HcAA==")
                    .unwrap(),
            ),
        ];

        // Matching an empty spec
        assert!(empty_filter.matches(&addr0, &[]).unwrap());

        assert!(empty_filter.matches(&addr0, &entries0).unwrap());

        // Matching the given address 0
        let filter0 = ParsedFilter {
            tipsets: ParsedFilterTipsets::Range(0..=0),
            addresses: vec![addr0],
            keys: Default::default(),
        };

        assert!(filter0.matches(&addr0, &[]).unwrap());

        assert!(!filter0.matches(&addr1, &[]).unwrap());

        // Matching the given address 0 or 1
        let filter1 = ParsedFilter {
            tipsets: ParsedFilterTipsets::Range(0..=0),
            addresses: vec![addr0, addr1],
            keys: Default::default(),
        };

        assert!(filter1.matches(&addr0, &[]).unwrap());

        assert!(filter1.matches(&addr1, &[]).unwrap());
    }

    #[test]
    fn test_parsed_filter_match_keys() {
        // Note that all the following addresses and topics (base64-encoded strings) come from real data on Calibnet,
        // but they could also be newly generated addresses or valid topic data.

        let addr0 = Address::from_str("t410f744ma4xsq3r3eczzktfj7goal67myzfkusna2hy").unwrap();

        let entries0 = vec![
            Entry::new(
                Flags::FLAG_INDEXED_ALL,
                "t1".into(),
                IPLD_RAW,
                BASE64_STANDARD
                    .decode("4kcg9Fy3Ty1V8d7rtgmPUPELUR2rin1HxIGaCNzQuJU=")
                    .unwrap(),
            ),
            Entry::new(
                Flags::FLAG_INDEXED_ALL,
                "t2".into(),
                IPLD_RAW,
                BASE64_STANDARD
                    .decode("dATj0QTqeEHD2eb9IK3+mbStWGvAjY8706/viUzxhN4=")
                    .unwrap(),
            ),
            Entry::new(
                Flags::FLAG_INDEXED_ALL,
                "d".into(),
                IPLD_RAW,
                BASE64_STANDARD
                    .decode("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAGCFA6vK+FJsAAAAAAAAAAAAAAAAAAAAAAAAAAAAAFLvXoMoks6HcAA==")
                    .unwrap(),
            ),
        ];

        let empty_filter = ParsedFilter {
            tipsets: ParsedFilterTipsets::Range(0..=0),
            addresses: vec![],
            keys: Default::default(),
        };

        assert!(empty_filter.matches(&addr0, &entries0).unwrap());

        let mut keys: AHashMap<String, Vec<ActorEventBlock>> = Default::default();
        keys.insert(
            "t1".into(),
            vec![ActorEventBlock {
                codec: IPLD_RAW,
                value: BASE64_STANDARD
                    .decode("4kcg9Fy3Ty1V8d7rtgmPUPELUR2rin1HxIGaCNzQuJU=")
                    .unwrap(),
            }],
        );

        let filter1 = ParsedFilter {
            tipsets: ParsedFilterTipsets::Range(0..=0),
            addresses: vec![],
            keys,
        };

        assert!(filter1.matches(&addr0, &entries0).unwrap());

        let mut keys: AHashMap<String, Vec<ActorEventBlock>> = Default::default();
        keys.insert(
            "t1".into(),
            vec![ActorEventBlock {
                codec: IPLD_RAW,
                value: BASE64_STANDARD
                    .decode("0Gprf0kYSUs3GSF9GAJ4bB9REqbB2I/iz+wAtFhPauw=")
                    .unwrap(),
            }],
        );

        let filter2 = ParsedFilter {
            tipsets: ParsedFilterTipsets::Range(0..=0),
            addresses: vec![],
            keys,
        };

        assert!(!filter2.matches(&addr0, &entries0).unwrap());

        let mut keys: AHashMap<String, Vec<ActorEventBlock>> = Default::default();
        keys.insert(
            "t1".into(),
            vec![ActorEventBlock {
                codec: DAG_CBOR,
                value: BASE64_STANDARD
                    .decode("4kcg9Fy3Ty1V8d7rtgmPUPELUR2rin1HxIGaCNzQuJU=")
                    .unwrap(),
            }],
        );

        let filter2 = ParsedFilter {
            tipsets: ParsedFilterTipsets::Range(0..=0),
            addresses: vec![],
            keys,
        };

        assert!(!filter2.matches(&addr0, &entries0).unwrap());

        let mut keys: AHashMap<String, Vec<ActorEventBlock>> = Default::default();
        keys.insert(
            "t1".into(),
            vec![ActorEventBlock {
                codec: IPLD_RAW,
                value: BASE64_STANDARD
                    .decode("4kcg9Fy3Ty1V8d7rtgmPUPELUR2rin1HxIGaCNzQuJU=")
                    .unwrap(),
            }],
        );
        keys.insert(
            "t2".into(),
            vec![ActorEventBlock {
                codec: IPLD_RAW,
                value: BASE64_STANDARD
                    .decode("4kcg9Fy3Ty1V8d7rtgmPUPELUR2rin1HxIGaCNzQuJU=")
                    .unwrap(),
            }],
        );

        let filter3 = ParsedFilter {
            tipsets: ParsedFilterTipsets::Range(0..=0),
            addresses: vec![],
            keys,
        };

        assert!(!filter3.matches(&addr0, &entries0).unwrap());
    }
}
