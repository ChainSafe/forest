// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

use super::{Error, TipIndex};
use blocks::{BlockHeader, RawBlock, Tipset};
use cid::Cid;
use db::{Error as DbError, Read, RocksDb as Blockstore, Write};
use encoding::from_slice;
use network::service::NetworkMessage;
use num_bigint::BigUint;

pub struct ChainStore {
    // TODO add IPLD Store
    // TODO add StateTreeLoader

    // key-value datastore
    db: Blockstore,

    // CID of the genesis block.
    genesis: Cid,

    // Tipset at the head of the best-known chain.
    heaviest: Tipset,

    // A pubsub channel that publishes an event every time the head changes.
    _notifications: NetworkMessage,

    // tip_index tracks tipsets by epoch/parentset for use by expected consensus.
    _tip_index: TipIndex,
}

impl ChainStore {
    pub fn weight(&self, _ts: &Tipset) -> Result<BigUint, Error> {
        // TODO
        Ok(BigUint::from(0 as u32))
    }
    /// Writes genesis to blockstore
    pub fn set_genesis(&self, header: BlockHeader) -> Result<(), DbError> {
        let ts: Tipset = Tipset::new(vec![header])?;
        Ok(self.persist_headers(&ts)?)
    }
    /// Writes encoded blockheader data to blockstore
    pub fn persist_headers(&self, tip: &Tipset) -> Result<(), DbError> {
        let mut raw_header_data = Vec::new();
        let mut keys = Vec::new();
        for i in 0..tip.blocks().len() {
            if !self.db.exists(tip.blocks[i].cid().key())? {
                raw_header_data.push(tip.blocks[i].raw_data()?);
                keys.push(tip.blocks[i].cid().key())
            }
        }
        Ok(self.db.bulk_write(&keys, &raw_header_data)?)
    }
    /// Returns genesis blockheader from blockstore
    pub fn get_genesis(&self) -> Result<BlockHeader, Error> {
        let bz = self.db.read(self.genesis.key())?;
        from_slice(&bz.unwrap())?
    }
    /// Returns heaviest tipset from blockstore
    pub fn get_heaviest_tipset(&self) -> &Tipset {
        &self.heaviest
    }
}
