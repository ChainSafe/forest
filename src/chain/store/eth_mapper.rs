// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use ahash::HashMap;
use std::sync::Arc;

use cid::Cid;
use fvm_ipld_blockstore::Blockstore;

use super::Error;
use crate::blocks::Tipset;
use crate::chain::block_messages;
use crate::message::SignedMessage;
use crate::networks::{ChainConfig, Height};
use crate::rpc::eth::{eth_tx_from_signed_eth_message, Hash};

#[derive(Default)]
pub struct EthMapper<DB> {
    db: DB,
    chain_config: Arc<ChainConfig>,
}

impl<DB: Blockstore> EthMapper<DB> {
    pub fn new(db: DB, chain_config: Arc<ChainConfig>) -> Self {
        Self { db, chain_config }
    }

    pub fn populate(&self, tipset: &Tipset) -> anyhow::Result<()> {
        let min_height = self.chain_config.epoch(Height::Hygge);

        let mut curr = tipset.clone();
        let mut signed_messages = vec![];
        while curr.epoch() > min_height {
            let ts = Tipset::load_required(&self.db, curr.parents())?;
            for bh in ts.block_headers() {
                if let Ok((_, mut secp_cids)) = block_messages(&self.db, bh) {
                    signed_messages.append(&mut secp_cids);
                }
            }
            curr = ts;
        }
        tracing::debug!("Processing {} messages", signed_messages.len());
        let i = self.process_signed_messages(&signed_messages)?;

        tracing::debug!("Cached {} entries", i);

        Ok(())
    }

    fn process_signed_messages(&self, messages: &[SignedMessage]) -> anyhow::Result<usize> {
        let delegated_messages = messages.iter().filter(|msg| msg.is_delegated());
        let eth_chain_id = self.chain_config.eth_chain_id;

        let eth_txs: Vec<(Hash, Cid, usize)> = delegated_messages
            .enumerate()
            .filter_map(|(i, smsg)| {
                //
                if let Ok(tx) = eth_tx_from_signed_eth_message(smsg, eth_chain_id) {
                    if let Ok(hash) = tx.eth_hash() {
                        // newest messages are the ones with lowest index
                        Some((hash, smsg.cid().unwrap(), i))
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect();
        let filtered = filter_lowest_index(eth_txs);

        // write back
        let n = filtered.len();
        for (k, v) in filtered.into_iter() {
            let block = &fvm_ipld_encoding::to_vec(&v)?;
            self.db.put_keyed(&k.to_cid(), block)?;
        }
        Ok(n)
    }

    pub fn get_cid_from_hash(&self, hash: &Hash) -> anyhow::Result<Cid, Error> {
        let bytes = self
            .db
            .get(&hash.to_cid())?
            .ok_or_else(|| Error::UndefinedKey(hash.to_string()))?;
        Ok(fvm_ipld_encoding::from_slice::<Cid>(&bytes)?)
    }
}

fn filter_lowest_index(values: Vec<(Hash, Cid, usize)>) -> Vec<(Hash, Cid)> {
    let map: HashMap<Hash, (Cid, usize)> =
        values
            .into_iter()
            .fold(HashMap::default(), |mut acc, (hash, cid, index)| {
                acc.entry(hash)
                    .and_modify(|&mut (_, ref mut min_index)| {
                        if index < *min_index {
                            *min_index = index;
                        }
                    })
                    .or_insert((cid, index));
                acc
            });

    map.into_iter()
        .map(|(hash, (cid, _))| (hash, cid))
        .collect()
}
