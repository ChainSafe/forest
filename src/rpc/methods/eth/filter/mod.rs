// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod event;
mod store;
mod tipset;

use crate::rpc::eth::filter::event::*;
use crate::rpc::eth::filter::tipset::*;
use crate::rpc::eth::types::*;
use crate::shim::address::Address;
use crate::shim::clock::ChainEpoch;
use crate::utils::misc::env::env_or_default;
use ahash::AHashMap as HashMap;
use anyhow::{anyhow, Context, Error};
use cid::Cid;
use serde::*;
use std::sync::Arc;
use store::*;

pub struct EthEventHandler {
    filter_store: Option<Arc<dyn FilterStore>>,
    max_filter_height_range: ChainEpoch,
    event_filter_manager: Option<Arc<EventFilterManager>>,
    tipset_filter_manager: Option<Arc<TipSetFilterManager>>,
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

        Self {
            filter_store,
            max_filter_height_range,
            event_filter_manager,
            tipset_filter_manager,
        }
    }

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
}

impl EthFilterSpec {
    fn parse_eth_filter_spec(
        &self,
        chain_height: i64,
        max_filter_height_range: i64,
    ) -> Result<ParsedFilter, Error> {
        let (min_height, max_height, tipset_cid) = if let Some(block_hash) = &self.block_hash {
            if self.from_block.is_some() || self.to_block.is_some() {
                return Err(anyhow!("must not specify block hash and from/to block"));
            }
            (0, 0, block_hash.to_cid())
        } else {
            let (min, max) = parse_block_range(
                chain_height,
                self.from_block.as_deref(),
                self.to_block.as_deref(),
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
    from_block: Option<&str>,
    to_block: Option<&str>,
    max_range: ChainEpoch,
) -> Result<(ChainEpoch, ChainEpoch), Error> {
    let min_height = match from_block {
        None | Some("latest") | Some("") => heaviest,
        Some("earliest") => 0,
        Some(block) => {
            if !block.starts_with("0x") {
                return Err(anyhow!("FromBlock is not a hex"));
            }
            hex_str_to_epoch(block)?
        }
    };

    let max_height = match to_block {
        None | Some("latest") | Some("") => -1,
        Some("earliest") => 0,
        Some(block) => {
            if !block.starts_with("0x") {
                return Err(anyhow!("ToBlock is not a hex"));
            }
            hex_str_to_epoch(block)?
        }
    };

    if min_height == -1 && max_height > 0 {
        if max_height - heaviest > max_range {
            return Err(anyhow!(
                "invalid epoch range: to block is too far in the future (maximum: {})",
                max_range
            ));
        }
    } else if min_height >= 0 && max_height == -1 {
        if heaviest - min_height > max_range {
            return Err(anyhow!(
                "invalid epoch range: from block is too far in the past (maximum: {})",
                max_range
            ));
        }
    } else if min_height >= 0 && max_height >= 0 {
        if min_height > max_height {
            return Err(anyhow!(
                "invalid epoch range: to block ({}) must be after from block ({})",
                max_height,
                min_height
            ));
        } else if max_height - min_height > max_range {
            return Err(anyhow!(
                "invalid epoch range: range between to and from blocks is too large (maximum: {})",
                max_range
            ));
        }
    }

    Ok((min_height, max_height))
}

fn hex_str_to_epoch(hex_str: &str) -> Result<ChainEpoch, Error> {
    let hex_substring = hex_str.strip_prefix("0x").unwrap_or(hex_str);
    i64::from_str_radix(hex_substring, 16).map_err(|e| anyhow!(e.to_string()))
}

fn parse_eth_topics(topics: &EthTopicSpec) -> Result<HashMap<String, Vec<Vec<u8>>>, Error> {
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

    for (key, val) in keys {
        for v in val {
            keys_with_codec
                .entry(key.clone())
                .or_default()
                .push(ActorEventBlock {
                    codec: MULTICODEC_RAW,
                    value: v,
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
    use super::*;
    use crate::rpc::eth::{EthFilterSpec, EthAddress, EthTopicSpec};
    use std::str::FromStr;

    #[test]
    fn test_parse_eth_filter_spec() {
        let eth_filter_spec = EthFilterSpec {
            from_block: Some("earliest".into()),
            to_block: Some("latest".into()),
            address: vec![EthAddress::from_str(
                "0xff38c072f286e3b20b3954ca9f99c05fbecc64aa",
            )
            .unwrap()],
            topics: EthTopicSpec(vec![]),
            block_hash: None,
        };

        let chain_height = 50;
        let max_filter_height_range = 100;

        assert!(eth_filter_spec.parse_eth_filter_spec(chain_height, max_filter_height_range).is_ok());
    }

    #[test]
    fn test_parse_block_range() {
        let heaviest = 50;
        let max_range = 100;

        // Test case 1: from_block = "earliest", to_block = "latest"
        let result = parse_block_range(heaviest, Some("earliest"), Some("latest"), max_range);
        assert!(result.is_ok());
        let (min_height, max_height) = result.unwrap();
        assert_eq!(min_height, 0);
        assert_eq!(max_height, -1);

        // Test case 2: from_block = "0x1", to_block = "0xA"
        let result = parse_block_range(heaviest, Some("0x1"), Some("0xA"), max_range);
        assert!(result.is_ok());
        let (min_height, max_height) = result.unwrap();
        assert_eq!(min_height, 1);  // hex_str_to_epoch("0x1") = 1
        assert_eq!(max_height, 10); // hex_str_to_epoch("0xA") = 10

        // Test case 3: from_block = "latest", to_block = None
        let result = parse_block_range(heaviest, Some("latest"), None, max_range);
        assert!(result.is_ok());
        let (min_height, max_height) = result.unwrap();
        assert_eq!(min_height, heaviest);
        assert_eq!(max_height, -1);

        // Test case 4: Invalid block hex format
        let result = parse_block_range(heaviest, Some("invalid"), Some("0xA"), max_range);
        assert!(result.is_err());

        // Test case 5: Range too large
        let result = parse_block_range(heaviest, Some("earliest"), Some("0x100"), max_range);
        assert!(result.is_err());
    }
}
