// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::prelude::*;
use crate::rpc::eth::{
    CollectedEvent,
    filter::{ActorEventBlock, ParsedFilter, ParsedFilterTipsets},
};
use crate::shim::address::Protocol;
use ahash::HashSet;
use sqlx::Arguments as _;
use std::borrow::Cow;

impl SqliteIndexer {
    pub async fn get_events_for_filter(
        &self,
        filter: IndexerEventFilter,
    ) -> anyhow::Result<Vec<CollectedEvent>> {
        let mut qb = filter.to_query_builder()?;
        let query = qb.build();
        let results = query.fetch_all(self.db()).await?;
        tracing::info!("results: {}, SQL: {}", results.len(), qb.into_string());
        Ok(vec![])
    }
}

#[derive(Debug, Clone, Default)]
pub struct IndexerEventFilter {
    pub min_height: ChainEpoch,
    pub max_height: ChainEpoch,
    pub tipset_cid: Option<Cid>,
    pub msg_cid: Option<Cid>,
    pub addresses: Vec<Address>,
    pub keys: HashMap<String, Vec<ActorEventBlock>>,
}

impl IndexerEventFilter {
    pub fn to_query_builder(&self) -> anyhow::Result<sqlx::QueryBuilder<sqlx::Sqlite>> {
        let arg_err = |e| anyhow::anyhow!("failed to push argument: {e}");

        let mut clauses: Vec<Cow<'static, str>> = vec![];
        let mut joins = vec![];
        let mut args = sqlx::sqlite::SqliteArguments::default();
        if let Some(ts_cid) = self.tipset_cid {
            clauses.push("tm.tipset_key_cid=?".into());
            args.add(ts_cid.to_bytes()).map_err(arg_err)?;
        } else if self.min_height >= 0 && self.min_height == self.max_height {
            clauses.push("tm.height=?".into());
            args.add(self.min_height).map_err(arg_err)?;
        } else if self.min_height >= 0 && self.max_height >= 0 {
            anyhow::ensure!(
                self.max_height >= self.min_height,
                "max_height should not be less that min_height"
            );
            clauses.push("tm.height BETWEEN ? AND ?".into());
            args.add(self.min_height).map_err(arg_err)?;
            args.add(self.max_height).map_err(arg_err)?;
        } else if self.min_height >= 0 {
            clauses.push("tm.height >= ?".into());
            args.add(self.min_height).map_err(arg_err)?;
        } else if self.max_height >= 0 {
            clauses.push("tm.height <= ?".into());
            args.add(self.max_height).map_err(arg_err)?;
        } else {
            anyhow::bail!("filter must specify either a tipset or a height range");
        }
        // unless asking for a specific tipset, we never want to see reverted historical events
        clauses.push("e.reverted=?".into());
        args.add(false).map_err(arg_err)?;

        if let Some(msg_cid) = self.msg_cid {
            clauses.push("tm.message_cid=?".into());
            args.add(msg_cid.to_bytes()).map_err(arg_err)?;
        }

        if !self.addresses.is_empty() {
            let mut id_addresses = HashSet::default();
            let mut delegated_addresses = HashSet::default();
            for addr in self.addresses.iter() {
                match addr.protocol() {
                    Protocol::ID => {
                        id_addresses.insert(addr.id()?);
                    }
                    Protocol::Delegated => {
                        delegated_addresses.insert(addr.to_bytes());
                    }
                    p => {
                        anyhow::bail!(
                            "can only query events by ID or Delegated addresses; but request has {p} address"
                        );
                    }
                }
            }

            if !id_addresses.is_empty() {
                let placeholders = id_addresses.iter().map(|_| "?").join(",");
                clauses.push(format!("e.emitter_id IN ({placeholders})").into());
                for id in id_addresses {
                    args.add(id as i64).map_err(arg_err)?;
                }
            }

            if !delegated_addresses.is_empty() {
                let placeholders = delegated_addresses.iter().map(|_| "?").join(",");
                clauses.push(format!("e.emitter_addr IN ({placeholders})").into());
                for addr in delegated_addresses {
                    args.add(addr).map_err(arg_err)?;
                }
            }
        }

        // join
        if !self.keys.is_empty() {
            let mut idx = 0;
            for (key, vals) in self.keys.iter() {
                if !vals.is_empty() {
                    idx += 1;
                    let alias = format!("ee{idx}");
                    joins.push(format!("event_entry {alias} ON e.id={alias}.event_id"));
                    clauses.push(format!("{alias}.indexed=1 AND {alias}.key=?").into());
                    args.add(key).map_err(arg_err)?;
                    let mut subclauses = Vec::with_capacity(vals.len());
                    for val in vals {
                        subclauses.push(format!("({alias}.codec=? AND {alias}.value=?)"));
                        args.add(val.codec as i64).map_err(arg_err)?;
                        args.add(&val.value).map_err(arg_err)?;
                    }
                    clauses.push(format!("({})", subclauses.join(" OR ")).into());
                }
            }
        }

        let mut qb = sqlx::QueryBuilder::with_arguments(
            "SELECT
			e.id,
			tm.height,
			tm.tipset_key_cid,
			e.emitter_id,
			e.emitter_addr,
			e.event_index,
			tm.message_cid,
			tm.message_index,
			e.reverted,
			ee.flags,
			ee.key,
			ee.codec,
			ee.value
            FROM event e
            JOIN tipset_message tm ON e.message_id = tm.id
            JOIN event_entry ee ON e.id = ee.event_id",
            args,
        );

        // join
        if !joins.is_empty() {
            qb.push(format!(", {}", joins.join(", ")));
        }

        // where
        if !clauses.is_empty() {
            qb.push(format!(" WHERE {}", clauses.join(" AND ")));
        }

        // order: retain insertion order of event_entry rows
        qb.push(" ORDER BY tm.height ASC, tm.message_index ASC, e.event_index ASC, ee._rowid_ ASC");
        Ok(qb)
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn t1() {
        let f = IndexerEventFilter {
            tipset_cid: Some(Cid::default()),
            ..Default::default()
        };
        let qb = f.to_query_builder().unwrap();
        println!("{:?}", qb.into_sql());
    }
}
