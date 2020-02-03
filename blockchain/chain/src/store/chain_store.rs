// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{Error, TipIndex, TipSetMetadata};
use blocks::{BlockHeader, Tipset};
use cid::Cid;
use db::{Error as DbError, Read, RocksDb as Blockstore, Write};
use encoding::from_slice;
use num_bigint::BigUint;
use raw_block::RawBlock;
use std::path::Path;

/// Generic implementation of the datastore trait and structures
pub struct ChainStore<'a> {
    // TODO add IPLD Store
    // TODO add StateTreeLoader
    // TODO add a pubsub channel that publishes an event every time the head changes.

    // key-value datastore
    db: Blockstore,

    // CID of the genesis block.
    genesis: Cid,

    // Tipset at the head of the best-known chain.
    heaviest: &'a Tipset,

    // tip_index tracks tipsets by epoch/parentset for use by expected consensus.
    tip_index: TipIndex,
}

impl<'a> ChainStore<'a> {
    /// constructor
    pub fn new(path: &Path, gen: Cid, heaviest: &'a Tipset) -> Result<Self, Error> {
        let mut db = Blockstore::new(path.to_path_buf());
        // initialize key-value store
        db.open()?;

        Ok(Self {
            db,
            tip_index: TipIndex::new(),
            genesis: gen,
            heaviest,
        })
    }
    /// Sets tip_index tracker
    pub fn set_tipset_tracker(&mut self, header: &BlockHeader) -> Result<(), Error> {
        let ts: Tipset = Tipset::new(vec![header.clone()])?;
        let meta = TipSetMetadata {
            tipset_state_root: header.state_root().clone(),
            tipset_receipts_root: header.message_receipts().clone(),
            tipset: ts,
        };
        Ok(self.tip_index.put(&meta)?)
    }
    /// weight
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
    /// Writes encoded message data to blockstore
    pub fn put_messages<T: RawBlock>(&self, msgs: &[T]) -> Result<(), Error> {
        for m in msgs {
            let key = m.cid()?.key();
            let value = &m.raw_data()?;
            if self.db.exists(&key)? {
                return Err(Error::KeyValueStore("Keys exist".to_string()));
            }
            self.db.write(&key, value)?
        }
        Ok(())
    }
    /// Returns genesis blockheader from blockstore
    pub fn genesis(&self) -> Result<BlockHeader, Error> {
        let bz = self.db.read(self.genesis.key())?;
        match bz {
            None => Err(Error::UndefinedKey(
                "Genesis key does not exist".to_string(),
            )),
            Some(ref x) => from_slice(&x)?,
        }
    }
    /// Returns heaviest tipset from blockstore
    pub fn heaviest_tipset(&self) -> &Tipset {
        &self.heaviest
    }
    /// Returns key-value store instance
    pub fn blockstore(&self) -> &Blockstore {
        &self.db
    }
}
