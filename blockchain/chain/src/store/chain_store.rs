// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{Error, TipIndex, TipSetMetadata};
use blocks::{Block, BlockHeader, FullTipset, TipSetKeys, Tipset, TxMeta};
use cid::Cid;
use encoding::{de::DeserializeOwned, from_slice, Cbor};
use ipld_amt::Amt;
use ipld_blockstore::BlockStore;
use message::{SignedMessage, UnsignedMessage};
use num_bigint::BigUint;
use std::sync::Arc;

const GENESIS_KEY: &str = "gen_block";

/// Generic implementation of the datastore trait and structures
pub struct ChainStore<DB> {
    // TODO add IPLD Store
    // TODO add StateTreeLoader
    // TODO add a pubsub channel that publishes an event every time the head changes.

    // key-value datastore
    pub db: Arc<DB>,

    // Tipset at the head of the best-known chain.
    heaviest: Arc<Tipset>,

    // tip_index tracks tipsets by epoch/parentset for use by expected consensus.
    tip_index: TipIndex,
}

impl<DB> ChainStore<DB>
where
    DB: BlockStore,
{
    /// constructor
    pub fn new(db: Arc<DB>) -> Self {
        // TODO pull heaviest tipset from data storage
        let heaviest = Arc::new(Tipset::new(vec![BlockHeader::default()]).unwrap());
        Self {
            db,
            tip_index: TipIndex::new(),
            heaviest,
        }
    }

    /// Sets heaviest tipset within ChainStore
    pub fn set_heaviest_tipset(&mut self, ts: Arc<Tipset>) {
        self.heaviest = ts;
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
    pub fn set_genesis(&self, header: BlockHeader) -> Result<(), Error> {
        self.db.write(GENESIS_KEY, header.marshal_cbor()?)?;
        let ts: Tipset = Tipset::new(vec![header])?;
        Ok(self.persist_headers(&ts)?)
    }

    /// Writes encoded blockheader data to blockstore
    pub fn persist_headers(&self, tip: &Tipset) -> Result<(), Error> {
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
                return Ok(());
            }
            self.db.write(&key, value)?
        }
        Ok(())
    }

    /// Returns genesis blockheader from blockstore
    pub fn genesis(&self) -> Result<Option<BlockHeader>, Error> {
        Ok(match self.db.read(GENESIS_KEY)? {
            Some(bz) => Some(BlockHeader::unmarshal_cbor(&bz)?),
            None => None,
        })
    }

    /// Returns heaviest tipset from blockstore
    pub fn heaviest_tipset(&self) -> Arc<Tipset> {
        self.heaviest.clone()
    }

    /// Returns key-value store instance
    pub fn blockstore(&self) -> &DB {
        &self.db
    }

    /// Returns Tipset from key-value store from provided cids
    pub fn tipset_from_keys(&self, tsk: &TipSetKeys) -> Result<Tipset, Error> {
        let mut block_headers = Vec::new();
        for c in tsk.cids() {
            let raw_header = self.db.read(c.key())?;
            if let Some(x) = raw_header {
                // decode raw header into BlockHeader
                let bh = BlockHeader::unmarshal_cbor(&x)?;
                block_headers.push(bh);
            } else {
                return Err(Error::NotFound("Key for header"));
            }
        }
        // construct new Tipset to return
        let ts = Tipset::new(block_headers)?;
        Ok(ts)
    }

    /// Returns a tuple of cids for both Unsigned and Signed messages
    fn read_msg_cids(&self, msg_cid: &Cid) -> Result<(Vec<Cid>, Vec<Cid>), Error> {
        if let Some(roots) = self
            .blockstore()
            .get::<TxMeta>(msg_cid)
            .map_err(|e| Error::Other(e.to_string()))?
        {
            let bls_cids = self.read_amt_cids(&roots.bls_message_root)?;
            let secpk_cids = self.read_amt_cids(&roots.secp_message_root)?;
            Ok((bls_cids, secpk_cids))
        } else {
            Err(Error::UndefinedKey("no msgs with that key".to_string()))
        }
    }

    /// Returns a vector of cids from provided root cid
    fn read_amt_cids(&self, root: &Cid) -> Result<Vec<Cid>, Error> {
        let amt = Amt::load(root, self.blockstore())?;

        let mut cids = Vec::new();
        for i in 0..amt.count() {
            if let Some(c) = amt.get(i)? {
                cids.push(c);
            }
        }

        Ok(cids)
    }

    /// Returns a Tuple of bls messages of type UnsignedMessage and secp messages
    /// of type SignedMessage
    pub fn messages(
        &self,
        bh: &BlockHeader,
    ) -> Result<(Vec<UnsignedMessage>, Vec<SignedMessage>), Error> {
        let (bls_cids, secpk_cids) = self.read_msg_cids(bh.messages())?;

        let bls_msgs: Vec<UnsignedMessage> = self.messages_from_cids(bls_cids)?;
        let secp_msgs: Vec<SignedMessage> = self.messages_from_cids(secpk_cids)?;

        Ok((bls_msgs, secp_msgs))
    }

    /// Returns messages from key-value store
    pub fn messages_from_cids<T>(&self, keys: Vec<Cid>) -> Result<Vec<T>, Error>
    where
        T: DeserializeOwned,
    {
        keys.iter()
            .map(|k| {
                let value = self.db.read(&k.key())?;
                let bytes = value.ok_or_else(|| Error::UndefinedKey(k.to_string()))?;

                // Decode bytes into type T
                let t = from_slice(&bytes)?;
                Ok(t)
            })
            .collect()
    }

    /// Constructs and returns a full tipset if messages from storage exists
    pub fn fill_tipsets(&self, ts: Tipset) -> Result<FullTipset, Error> {
        let mut blocks: Vec<Block> = Vec::with_capacity(ts.blocks().len());

        for header in ts.blocks() {
            let (bls_messages, secp_messages) = self.messages(header)?;
            blocks.push(Block {
                header: header.clone(),
                bls_messages,
                secp_messages,
            });
        }

        Ok(FullTipset::new(blocks))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn genesis_test() {
        let db = db::MemoryDB::default();

        let cs = ChainStore::new(Arc::new(db));
        let gen_block = BlockHeader::builder()
            .epoch(1)
            .weight((2 as u32).into())
            .build_and_validate()
            .unwrap();

        assert_eq!(cs.genesis().unwrap(), None);
        cs.set_genesis(gen_block.clone()).unwrap();
        assert_eq!(cs.genesis().unwrap(), Some(gen_block));
    }
}
