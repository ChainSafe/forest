// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{Error, TipIndex, TipSetMetadata};
use blocks::{BlockHeader, Tipset};
use cid::Cid;
use db::{Error as DbError, Read, RocksDb as Blockstore, Write};
use encoding::{de::DeserializeOwned, from_slice, Cbor};
use message::{SignedMessage, UnsignedMessage};
use num_bigint::BigUint;
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
                raw_header_data.push(block.marshal_cbor()?);
                keys.push(block.cid().key());
            }
        }
        Ok(self.db.bulk_write(&keys, &raw_header_data)?)
    }
    /// Writes encoded message data to blockstore
    pub fn put_messages<T: Cbor>(&self, msgs: &[T]) -> Result<(), Error> {
        for m in msgs {
            let key = m.cid()?.key();
            let value = &m.marshal_cbor()?;
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
    /// Returns Tipset from key-value store from provided cids
    pub fn tipset(&self, cids: &[Cid]) -> Result<Tipset, Error> {
        let mut block_headers = Vec::new();
        for c in cids {
            let raw_header = self.db.read(c.key())?;
            if let Some(x) = raw_header {
                // decode raw header into BlockHeader
                let bh = from_slice(&x)?;
                block_headers.push(bh);
            } else {
                return Err(Error::KeyValueStore(
                    "Key for header does not exist".to_string(),
                ));
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
        _bh: &BlockHeader,
    ) -> Result<(Vec<UnsignedMessage>, Vec<SignedMessage>), Error> {
        // TODO dependent on HAMT

        let bls_msgs: Vec<UnsignedMessage> = self.messages_from_cids(Vec::new())?;
        let secp_msgs: Vec<SignedMessage> = self.messages_from_cids(Vec::new())?;
        Ok((bls_msgs, secp_msgs))
    }
    /// Returns messages from key-value store
    pub fn messages_from_cids<T>(&self, keys: Vec<&Cid>) -> Result<Vec<T>, Error>
    where
        T: DeserializeOwned,
    {
        keys.iter()
            .map(|k| {
                let value = self.db.read(&k.key())?;
                let bytes = value.ok_or_else(|| Error::UndefinedKey(k.to_string()))?;

                // Decode bytes into type T
                from_slice(&bytes)?
            })
            .collect()
    }
}
