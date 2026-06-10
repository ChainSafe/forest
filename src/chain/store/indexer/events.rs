// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::rpc::eth::filter::{ActorEventBlock, ParsedFilter, ParsedFilterTipsets};

impl SqliteIndexer {
    pub async fn get_events_for_filter(filter: &IndexerEventFilter) {}
}

pub struct IndexerEventFilter {
    pub min_height: ChainEpoch,
    pub max_height: ChainEpoch,
    pub tipset_cid: Option<Cid>,
    pub msg_cid: Option<Cid>,
    pub addresses: Vec<Address>,
    pub keys: HashMap<String, Vec<ActorEventBlock>>,
}

impl IndexerEventFilter {
    fn prefill_filter_query(&self) {}
}

impl TryFrom<ParsedFilter> for IndexerEventFilter {
    type Error = anyhow::Error;

    fn try_from(
        ParsedFilter {
            tipsets,
            addresses,
            keys,
            msg_cid,
        }: ParsedFilter,
    ) -> Result<Self, Self::Error> {
        let (min_height, max_height, tipset_cid) = match tipsets {
            ParsedFilterTipsets::Hash(h) => (-1, -1, Some(h.to_cid())),
            ParsedFilterTipsets::Range(mut r) => {
                let first = r.next().unwrap_or(-1);
                let last = r.last().unwrap_or(first);
                (first, last, None)
            }
            ParsedFilterTipsets::Key(k) => (-1, -1, Some(k.cid()?)),
        };
        Ok(Self {
            min_height,
            max_height,
            tipset_cid,
            msg_cid,
            addresses,
            keys,
        })
    }
}
