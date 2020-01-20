// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

use super::{Error, TipIndex};
use blocks::{BlockHeader, RawBlock, Tipset};
use cid::Cid;
use db::{Error as DbError, Read, RocksDb as Blockstore, Write};
use encoding::from_slice;
use num_bigint::BigUint;

#[derive(Default)]
pub struct ChainStore {
    // TODO add IPLD Store
    // TODO add StateTreeLoader
    // TODO add a pubsub channel that publishes an event every time the head changes.

    // key-value datastore
    db: Blockstore,

    // CID of the genesis block.
    genesis: Cid,

    // Tipset at the head of the best-known chain.
    heaviest: Tipset,

    // tip_index tracks tipsets by epoch/parentset for use by expected consensus.
    _tip_index: TipIndex,
}

impl ChainStore {
    /// TODO add constructor

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
        // loop through block to push blockheader raw data and cid into vector to be stored
        for block in tip.blocks() {
            if !self.db.exists(block.cid().key())? {
                raw_header_data.push(block.raw_data()?);
                keys.push(block.cid().key());
            }
        }
        Ok(self.db.bulk_write(&keys, &raw_header_data)?)
    }
    /// Returns genesis blockheader from blockstore
    pub fn get_genesis(&self) -> Result<BlockHeader, Error> {
        let bz = self.db.read(self.genesis.key())?;
        match bz {
            None => Err(Error::UndefinedKey(
                "Genesis key does not exist".to_string(),
            )),
            Some(ref x) => from_slice(&x)?,
        }
    }
    /// Returns heaviest tipset from blockstore
    pub fn get_heaviest_tipset(&self) -> &Tipset {
        &self.heaviest
    }
}
