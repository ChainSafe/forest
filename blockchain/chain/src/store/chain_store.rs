// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{Error, TipIndex, TipsetMetadata};
use actor::{power::State as PowerState, STORAGE_POWER_ACTOR_ADDR};
use blocks::{Block, BlockHeader, FullTipset, Tipset, TipsetKeys, TxMeta};
use cid::multihash::Blake2b256;
use cid::Cid;
use encoding::{de::DeserializeOwned, from_slice, Cbor};
use ipld_amt::Amt;
use ipld_blockstore::BlockStore;
use log::{info, warn};
use message::{SignedMessage, UnsignedMessage};
use num_bigint::BigUint;
use num_traits::Zero;
use state_tree::StateTree;
use std::sync::Arc;

const GENESIS_KEY: &str = "gen_block";
const HEAD_KEY: &str = "head";
// constants for Weight calculation
/// The ratio of weight contributed by short-term vs long-term factors in a given round
const W_RATIO_NUM: u64 = 1;
const W_RATIO_DEN: u64 = 2;
/// Blocks epoch allowed
const BLOCKS_PER_EPOCH: u64 = 5;

/// Generic implementation of the datastore trait and structures
pub struct ChainStore<DB> {
    // TODO add IPLD Store
    // TODO add a pubsub channel that publishes an event every time the head changes.

    // key-value datastore
    pub db: Arc<DB>,

    // Tipset at the head of the best-known chain.
    heaviest: Option<Arc<Tipset>>,

    // tip_index tracks tipsets by epoch/parentset for use by expected consensus.
    tip_index: TipIndex,
}

impl<DB> ChainStore<DB>
where
    DB: BlockStore,
{
    /// constructor
    pub fn new(db: Arc<DB>) -> Self {
        let heaviest = get_heaviest_tipset(db.as_ref())
            .unwrap_or(None)
            .map(Arc::new);
        Self {
            db,
            tip_index: TipIndex::new(),
            heaviest,
        }
    }

    /// Sets heaviest tipset within ChainStore and store its tipset cids under HEAD_KEY
    pub fn set_heaviest_tipset(&mut self, ts: Arc<Tipset>) -> Result<(), Error> {
        self.db.write(HEAD_KEY, ts.key().marshal_cbor()?)?;
        self.heaviest = Some(ts);
        Ok(())
    }

    /// Sets tip_index tracker
    pub fn set_tipset_tracker(&mut self, header: &BlockHeader) -> Result<(), Error> {
        let ts: Tipset = Tipset::new(vec![header.clone()])?;
        let meta = TipsetMetadata {
            tipset_state_root: header.state_root().clone(),
            tipset_receipts_root: header.message_receipts().clone(),
            tipset: ts,
        };
        self.tip_index.put(&meta);
        Ok(())
    }

    /// Writes genesis to blockstore
    pub fn set_genesis(&self, header: BlockHeader) -> Result<(), Error> {
        set_genesis(self.blockstore(), header)
    }

    /// Writes tipset block headers to data store and updates heaviest tipset
    pub fn put_tipsets(&mut self, ts: &Tipset) -> Result<(), Error> {
        persist_headers(self.blockstore(), ts.blocks())?;
        // TODO determine if expanded tipset is required; see https://github.com/filecoin-project/lotus/blob/testnet/3/chain/store/store.go#L236
        self.update_heaviest(ts)?;
        Ok(())
    }

    /// Writes encoded message data to blockstore
    pub fn put_messages<T: Cbor>(&self, msgs: &[T]) -> Result<(), Error> {
        put_messages(self.blockstore(), msgs)
    }

    /// Loads heaviest tipset from datastore and sets as heaviest in chainstore
    pub fn load_heaviest_tipset(&mut self) -> Result<(), Error> {
        let heaviest_ts = get_heaviest_tipset(self.blockstore())?.ok_or_else(|| {
            warn!("No previous chain state found");
            Error::Other("No chain state found".to_owned())
        })?;

        // set as heaviest tipset
        self.heaviest = Some(Arc::new(heaviest_ts));
        Ok(())
    }

    /// Returns genesis blockheader from blockstore
    pub fn genesis(&self) -> Result<Option<BlockHeader>, Error> {
        genesis(self.blockstore())
    }

    /// Returns heaviest tipset from blockstore
    pub fn heaviest_tipset(&self) -> Option<Arc<Tipset>> {
        self.heaviest.clone()
    }

    /// Returns key-value store instance
    pub fn blockstore(&self) -> &DB {
        &self.db
    }

    /// Returns Tipset from key-value store from provided cids
    pub fn tipset_from_keys(&self, tsk: &TipsetKeys) -> Result<Tipset, Error> {
        tipset_from_keys(self.blockstore(), tsk)
    }

    /// Returns a tuple of cids for both Unsigned and Signed messages
    fn read_msg_cids(&self, msg_cid: &Cid) -> Result<(Vec<Cid>, Vec<Cid>), Error> {
        if let Some(roots) = self
            .blockstore()
            .get::<TxMeta>(msg_cid)
            .map_err(|e| Error::Other(e.to_string()))?
        {
            let bls_cids = read_amt_cids(self.blockstore(), &roots.bls_message_root)?;
            let secpk_cids = read_amt_cids(self.blockstore(), &roots.secp_message_root)?;
            Ok((bls_cids, secpk_cids))
        } else {
            Err(Error::UndefinedKey("no msgs with that key".to_string()))
        }
    }

    /// Returns a Tuple of bls messages of type UnsignedMessage and secp messages
    /// of type SignedMessage
    pub fn messages(
        &self,
        bh: &BlockHeader,
    ) -> Result<(Vec<UnsignedMessage>, Vec<SignedMessage>), Error> {
        let (bls_cids, secpk_cids) = self.read_msg_cids(bh.messages())?;

        let bls_msgs: Vec<UnsignedMessage> = messages_from_cids(self.blockstore(), bls_cids)?;
        let secp_msgs: Vec<SignedMessage> = messages_from_cids(self.blockstore(), secpk_cids)?;

        Ok((bls_msgs, secp_msgs))
    }

    /// Constructs and returns a full tipset if messages from storage exists
    pub fn fill_tipsets(&self, ts: Tipset) -> Result<FullTipset, Error> {
        let mut blocks: Vec<Block> = Vec::with_capacity(ts.blocks().len());

        for header in ts.into_blocks() {
            let (bls_messages, secp_messages) = self.messages(&header)?;
            blocks.push(Block {
                header,
                bls_messages,
                secp_messages,
            });
        }

        // the given tipset has already been verified, so this cannot fail
        Ok(FullTipset::new(blocks).unwrap())
    }
    /// Determines if provided tipset is heavier than existing known heaviest tipset
    fn update_heaviest(&mut self, ts: &Tipset) -> Result<(), Error> {
        match &self.heaviest {
            Some(heaviest) => {
                let new_weight = weight(self.blockstore(), ts)?;
                let curr_weight = weight(self.blockstore(), &heaviest)?;
                if new_weight > curr_weight {
                    // TODO potentially need to deal with re-orgs here
                    info!("New heaviest tipset");
                    self.set_heaviest_tipset(Arc::new(ts.clone()))?;
                }
            }
            None => {
                info!("set heaviest tipset");
                self.set_heaviest_tipset(Arc::new(ts.clone()))?;
            }
        }
        Ok(())
    }
}

fn set_genesis<DB>(db: &DB, header: BlockHeader) -> Result<(), Error>
where
    DB: BlockStore,
{
    db.write(GENESIS_KEY, header.marshal_cbor()?)?;
    Ok(persist_headers(db, &[header])?)
}

fn persist_headers<DB>(db: &DB, bh: &[BlockHeader]) -> Result<(), Error>
where
    DB: BlockStore,
{
    let mut raw_header_data = Vec::new();
    let mut keys = Vec::new();
    // loop through block to push blockheader raw data and cid into vector to be stored
    for header in bh {
        if !db.exists(header.cid().key())? {
            raw_header_data.push(header.marshal_cbor()?);
            keys.push(header.cid().key());
        }
    }

    Ok(db.bulk_write(&keys, &raw_header_data)?)
}

fn put_messages<DB, T: Cbor>(db: &DB, msgs: &[T]) -> Result<(), Error>
where
    DB: BlockStore,
{
    for m in msgs {
        db.put(m, Blake2b256)
            .map_err(|e| Error::Other(e.to_string()))?;
    }
    Ok(())
}

fn get_heaviest_tipset<DB>(db: &DB) -> Result<Option<Tipset>, Error>
where
    DB: BlockStore,
{
    match db.read(HEAD_KEY)? {
        Some(bz) => {
            let keys: Vec<Cid> = from_slice(&bz)?;
            Ok(Some(tipset_from_keys(db, &TipsetKeys::new(keys))?))
        }
        None => Ok(None),
    }
}

/// Returns Tipset from key-value store from provided cids
fn tipset_from_keys<DB>(db: &DB, tsk: &TipsetKeys) -> Result<Tipset, Error>
where
    DB: BlockStore,
{
    let mut block_headers = Vec::new();
    for c in tsk.cids() {
        let raw_header = db.read(c.key())?;
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

/// Returns a vector of cids from provided root cid
fn read_amt_cids<DB>(db: &DB, root: &Cid) -> Result<Vec<Cid>, Error>
where
    DB: BlockStore,
{
    let amt = Amt::load(root, db)?;

    let mut cids = Vec::new();
    for i in 0..amt.count() {
        if let Some(c) = amt.get(i)? {
            cids.push(c);
        }
    }

    Ok(cids)
}

fn genesis<DB>(db: &DB) -> Result<Option<BlockHeader>, Error>
where
    DB: BlockStore,
{
    Ok(match db.read(GENESIS_KEY)? {
        Some(bz) => Some(BlockHeader::unmarshal_cbor(&bz)?),
        None => None,
    })
}

/// Returns messages from key-value store
fn messages_from_cids<DB, T>(db: &DB, keys: Vec<Cid>) -> Result<Vec<T>, Error>
where
    DB: BlockStore,
    T: DeserializeOwned,
{
    keys.iter()
        .map(|k| {
            let value = db.read(&k.key())?;
            let bytes = value.ok_or_else(|| Error::UndefinedKey(k.to_string()))?;

            // Decode bytes into type T
            let t = from_slice(&bytes)?;
            Ok(t)
        })
        .collect()
}

/// Returns the weight of provided tipset
fn weight<DB>(db: &DB, ts: &Tipset) -> Result<BigUint, String>
where
    DB: BlockStore,
{
    let mut tpow = BigUint::zero();
    let state = StateTree::new_from_root(db, ts.parent_state())?;
    if let Some(act) = state.get_actor(&*STORAGE_POWER_ACTOR_ADDR)? {
        if let Some(state) = db
            .get::<PowerState>(&act.state)
            .map_err(|e| e.to_string())?
        {
            tpow = state.total_network_power;
        }
    }
    let log2_p = if tpow > BigUint::zero() {
        BigUint::from(tpow.bits() - 1)
    } else {
        return Err(
            "All power in the net is gone. You network might be disconnected, or the net is dead!"
                .to_owned(),
        );
    };

    let mut out = ts.weight() + (&log2_p << 8);
    let e_weight = ((log2_p * BigUint::from(ts.blocks().len())) * BigUint::from(W_RATIO_NUM)) << 8;
    let value = e_weight / (BigUint::from(BLOCKS_PER_EPOCH) * BigUint::from(W_RATIO_DEN));
    out += &value;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use address::Address;
    use cid::multihash::Identity;

    #[test]
    fn genesis_test() {
        let db = db::MemoryDB::default();

        let cs = ChainStore::new(Arc::new(db));
        let gen_block = BlockHeader::builder()
            .epoch(1)
            .weight((2 as u32).into())
            .messages(Cid::new_from_cbor(&[], Identity))
            .message_receipts(Cid::new_from_cbor(&[], Identity))
            .state_root(Cid::new_from_cbor(&[], Identity))
            .miner_address(Address::new_id(0))
            .build_and_validate()
            .unwrap();

        assert_eq!(cs.genesis().unwrap(), None);
        cs.set_genesis(gen_block.clone()).unwrap();
        assert_eq!(cs.genesis().unwrap(), Some(gen_block));
    }
}
