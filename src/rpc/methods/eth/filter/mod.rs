// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod event;
mod store;
mod tipset;

use super::eth_filter_logs_from_events;
use super::BlockNumberOrHash;
use super::EthLog;
use super::Predefined;
use crate::rpc::eth::filter::event::*;
use crate::rpc::eth::filter::tipset::*;
use crate::rpc::eth::types::*;
use crate::rpc::eth::CollectedEvent;
use crate::rpc::Ctx;
use crate::shim::address::Address;
use crate::shim::clock::ChainEpoch;
use crate::utils::misc::env::env_or_default;
use ahash::AHashMap as HashMap;
use anyhow::{anyhow, bail, ensure, Context, Error};
use cid::Cid;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::IPLD_RAW;
use serde::*;
use std::sync::Arc;
use store::*;

/// Handles Ethereum event filters, providing an interface for creating and managing filters.
///
/// The `EthEventHandler` structure is the central point for managing Ethereum filters,
/// including event filters and tipSet filters. It interacts with a filter store and manages
/// configurations such as the maximum filter height range and maximum filter results.
pub struct EthEventHandler {
    filter_store: Option<Arc<dyn FilterStore>>,
    max_filter_height_range: ChainEpoch,
    event_filter_manager: Option<Arc<EventFilterManager>>,
    tipset_filter_manager: Option<Arc<TipSetFilterManager>>,
}

impl EthEventHandler {
    pub fn new() -> Self {
        let max_filters: usize = env_or_default("FOREST_MAX_FILTERS", 100);
        let max_filter_results: usize = env_or_default("FOREST_MAX_FILTER_RESULTS", 10000);
        let max_filter_height_range: i64 = env_or_default("FOREST_MAX_FILTER_HEIGHT_RANGE", 2880);
        let filter_store: Option<Arc<dyn FilterStore>> =
            Some(MemFilterStore::new(max_filters) as Arc<dyn FilterStore>);
        let event_filter_manager = Some(EventFilterManager::new(max_filter_results));
        let tipset_filter_manager = Some(TipSetFilterManager::new(max_filter_results));

        Self {
            filter_store,
            max_filter_height_range,
            event_filter_manager,
            tipset_filter_manager,
        }
    }

    // Installs an eth filter based on given filter spec.
    pub fn eth_new_filter(
        &self,
        filter_spec: &EthFilterSpec,
        chain_height: i64,
    ) -> Result<FilterID, Error> {
        if self.filter_store.is_none() {
            return Err(Error::msg("NotSupported"));
        }

        if let Some(event_filter_manager) = &self.event_filter_manager {
            let pf = filter_spec
                .parse_eth_filter_spec(chain_height, self.max_filter_height_range)
                .context("Parsing error")?;

            let filter = event_filter_manager
                .install(pf)
                .context("Installation error")?;

            if let Some(filter_store) = &self.filter_store {
                if filter_store.add(filter.clone()).is_err() {
                    if let Some(tipset_filter_manager) = &self.tipset_filter_manager {
                        let _ = tipset_filter_manager.remove(filter.id());
                    }
                    return Err(Error::msg("Removal error"));
                }
            }
            Ok(filter.id().clone())
        } else {
            Err(Error::msg("NotSupported"))
        }
    }

    fn eth_get_events_for_filter<DB: Blockstore>(
        &self,
        ctx: &Ctx<DB>,
        spec: EthFilterSpec,
    ) -> anyhow::Result<Vec<CollectedEvent>> {
        let event_filter_manager = self
            .event_filter_manager
            .as_ref()
            .context("not supported")?
            .clone();

        let _event_index = event_filter_manager
            .event_index
            .as_ref()
            .context("cannot use eth_get_logs if historical event index is disabled")?
            .clone();

        let pf = self.parse_eth_filter_spec(ctx, &spec)?;
        // Should pf.tipset_cid be an Option?
        let mut max_height = pf.max_height;
        if max_height == -1 {
            // heaviest tipset doesn't have events because its messages haven't been executed yet
            max_height = ctx.chain_store().heaviest_tipset().epoch() - 1;
        }

        if max_height < 0 {
            bail!("max_height requested is less than 0");
        }

        // we can't return events for the heaviest tipset as the transactions in that tipset will be executed
        // in the next non-null tipset (because of Filecoin's "deferred execution" model)
        if max_height > ctx.chain_store().heaviest_tipset().epoch() - 1 {
            bail!("max_height requested is greater than the heaviest tipset");
        }

        self.wait_for_height_processed(ctx, max_height)?;
        // TODO: Ideally we should also check that events for the epoch at `pf.minheight` have been indexed
        // However, it is currently tricky to check/guarantee this for two reasons:
        // a) Event Index is not aware of null-blocks. This means that the Event Index wont be able to say whether the block at
        //    `pf.minheight` is a null block or whether it has no events
        // b) There can be holes in the index where events at certain epoch simply haven't been indexed because of edge cases around
        //    node restarts while indexing. This needs a long term "auto-repair"/"automated-backfilling" implementation in the index
        // So, for now, the best we can do is ensure that the event index has evenets for events at height >= `pf.maxHeight`

        // Create a temporary filter
        let filter = event_filter_manager
            .install(pf)
            .context("failed to install event filter")?;
        let events = filter.take_collected_events();

        if !event_filter_manager.remove(filter.id()) {
            bail!("failed to uninstall filter");
        }

        Ok(events)
    }

    fn wait_for_height_processed<DB: Blockstore>(
        &self,
        ctx: &Ctx<DB>,
        max_height: ChainEpoch,
    ) -> anyhow::Result<()> {
        todo!()
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

    fn get_eth_logs_for_block_and_transaction<DB: Blockstore>(
        &self,
        ctx: &Ctx<DB>,
        block_hash: &EthHash,
        tx_hash: &EthHash,
    ) -> anyhow::Result<Vec<EthLog>> {
        let events = self.eth_get_events_for_filter(
            ctx,
            EthFilterSpec {
                block_hash: Some(block_hash.clone()),
                ..EthFilterSpec::default()
            },
        )?;
        let logs = eth_filter_logs_from_events(ctx, &events)?;
        let filtered: Vec<EthLog> = logs
            .into_iter()
            .filter(|e| &e.transaction_hash == tx_hash)
            .collect();
        Ok(filtered)
    }
}

impl EthFilterSpec {
    fn parse_eth_filter_spec(
        &self,
        chain_height: i64,
        max_filter_height_range: i64,
    ) -> Result<ParsedFilter, Error> {
        let from_block = self.from_block.as_deref().unwrap_or("");
        let to_block = self.to_block.as_deref().unwrap_or("");
        let (min_height, max_height, tipset_cid) = if let Some(block_hash) = &self.block_hash {
            if self.from_block.is_some() || self.to_block.is_some() {
                bail!("must not specify block hash and from/to block");
            }
            (0, 0, block_hash.to_cid())
        } else {
            let (min, max) = parse_block_range(
                chain_height,
                BlockNumberOrHash::from_str(from_block)?,
                BlockNumberOrHash::from_str(to_block)?,
                max_filter_height_range,
            )?;
            (min, max, Cid::default())
        };

        let addresses: Vec<_> = self
            .address
            .iter()
            .map(|ea| {
                ea.to_filecoin_address()
                    .map_err(|e| anyhow!("invalid address {}", e))
            })
            .collect::<Result<Vec<_>, _>>()?;

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
        let EthHashList(hashes) = eth_hash_list;
        let key = format!("t{}", idx + 1);
        for eth_hash in hashes {
            let EthHash(bytes) = eth_hash;
            keys.entry(key.clone()).or_default().push(bytes.0.to_vec());
        }
    }
    Ok(keys)
}

#[derive(Debug, Serialize, Deserialize)]
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

struct ParsedFilter {
    min_height: ChainEpoch,
    max_height: ChainEpoch,
    tipset_cid: Cid,
    addresses: Vec<Address>,
    keys: HashMap<String, Vec<ActorEventBlock>>,
}

#[cfg(test)]
mod tests {
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
            topics: EthTopicSpec(vec![]),
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
            topics: EthTopicSpec(vec![]),
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
        let topics = EthTopicSpec(vec![EthHashList(vec![EthHash::default()])]);
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
}
