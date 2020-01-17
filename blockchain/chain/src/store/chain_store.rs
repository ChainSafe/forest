// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

use super::{Error, TipIndex};
use blocks::{BlockHeader, RawBlock, Tipset};
use cid::Cid;
use db::Error as DbError;
use db::RocksDb as Blockstore;
use db::{Read, Write};
use network::service::NetworkMessage;
use num_bigint::BigUint;

pub struct ChainStore {
    // TODO add IPLD Store
    // TODO add StateTreeLoader

    // key-value datastore
    db: Blockstore,

    // CID of the genesis block.
    _genesis: Cid,

    // Tipset at the head of the best-known chain.
    heaviest: Tipset,

    // A pubsub channel that publishes an event every time the head changes.
    _notifications: NetworkMessage,

    // tip_index tracks tipsets by epoch/parentset for use by expected consensus.
    _tip_index: TipIndex,
}

impl ChainStore {
    pub fn set_genesis(&self, _header: BlockHeader) {}
    pub fn weight(&self, _ts: &Tipset) -> Result<BigUint, Error> {
        // TODO
        Ok(BigUint::from(0 as u32))
    }
    pub fn persist_headers(&self, tip: &Tipset) -> Result<(), DbError> {
        let mut raw_header_data = Vec::new();
        let mut keys = Vec::new();
        for i in 0..tip.blocks().len() {
            if !self.db.exists(tip.blocks[i].cid().key())? {
                raw_header_data.push(
                    tip.blocks[i]
                        .raw_data()
                        .map_err(|_e| DbError::new("Cbor Error".to_string()))?,
                );
                keys.push(tip.blocks[i].cid().key())
            }
        }
        Ok(self.db.bulk_write(&keys, &raw_header_data)?)
    }
    pub fn get_heaviest_tipset(&self) -> &Tipset {
        &self.heaviest
    }
}
