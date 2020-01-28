// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

use super::{Error, TipIndex, TipSetMetadata};
use blocks::{BlockHeader, Tipset};
use cid::Cid;
use db::{Error as DbError, Read, RocksDb as Blockstore, Write};
use encoding::from_slice;
use message::{SignedMessage, UnsignedMessage};
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
    /// Returns Tipset from key-value store from provided keys
    pub fn tipset(&self, keys: &[Cid]) -> Result<Tipset, Error> {
        let mut block_headers = Vec::new();
        for k in keys {
            let raw_header = self.db.read(k.key())?;
            if let Some(ref x) = raw_header {
                // decode raw header into BlockHeader
                let bh = from_slice(&x)?;
                block_headers.push(bh);
            }
        }
        // construct new Tipset to return
        let ts = Tipset::new(block_headers)?;
        Ok(ts)
    }
    /// Returns a Tuple of bls messages of type UnsignedMessage and secp messages
    /// of type SignedMessage
    pub fn messages(
        &self,
        bh: &BlockHeader,
    ) -> Result<(Vec<UnsignedMessage>, Vec<SignedMessage>), Error> {
        // TODO read_msg_cids from message root; returns bls_cids and secp_cids
        // let (blscids, secpkcids) = read_msg_cids(bh.messages())
        // temporarily using vec!(bh.message_receipts() until read_msg_cids is completed with AMT/HAMT
        let bls_msgs: Vec<UnsignedMessage> = self.bls_messages(vec![bh.message_receipts()])?;
        let secp_msgs: Vec<SignedMessage> = self.secp_messages(vec![bh.message_receipts()])?;
        Ok((bls_msgs, secp_msgs))
    }
    /// Returns UnsignedMessages from key-value store
    pub fn bls_messages(&self, keys: Vec<&Cid>) -> Result<Vec<UnsignedMessage>, Error> {
        let mut block_messages = Vec::new();
        for k in keys {
            let raw_msgs = self.db.read(&k.key())?;
            if let Some(ref x) = raw_msgs {
                // decode raw messages into type UnsignedMessage
                let msg = from_slice(&x)?;
                block_messages.push(msg);
            }
        }
        Ok(block_messages)
    }
    /// Returns SignedMessages from key-value store
    pub fn secp_messages(&self, keys: Vec<&Cid>) -> Result<Vec<SignedMessage>, Error> {
        let mut block_messages = Vec::new();
        for k in keys {
            let raw_msgs = self.db.read(&k.key())?;
            if let Some(ref x) = raw_msgs {
                // decode raw messages into type SignedMessage
                let msg = from_slice(&x)?;
                block_messages.push(msg);
            }
        }
        Ok(block_messages)
    }
}
