// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{Error, TipIndex, TipsetMetadata};
use actor::{power::State as PowerState, STORAGE_POWER_ACTOR_ADDR};
use address::Address;
use beacon::BeaconEntry;
use blake2b_simd::Params;
use blocks::{Block, BlockHeader, FullTipset, Tipset, TipsetKeys, TxMeta};
use byteorder::{BigEndian, WriteBytesExt};
use cid::multihash::Blake2b256;
use cid::Cid;
use clock::ChainEpoch;
use crypto::DomainSeparationTag;
use encoding::{blake2b_256, de::DeserializeOwned, from_slice, Cbor};
use flo_stream::{MessagePublisher, Publisher, Subscriber};
use ipld_amt::Amt;
use ipld_blockstore::BlockStore;
use log::{info, warn};
use message::{ChainMessage, Message, MessageReceipt, SignedMessage, UnsignedMessage};
use num_bigint::{BigInt, Sign};
use num_traits::Zero;
use serde::Serialize;
use state_tree::StateTree;
use std::collections::HashMap;
use std::io::Write;
use std::sync::Arc;

const GENESIS_KEY: &str = "gen_block";
const HEAD_KEY: &str = "head";
// constants for Weight calculation
/// The ratio of weight contributed by short-term vs long-term factors in a given round
const W_RATIO_NUM: u64 = 1;
const W_RATIO_DEN: u64 = 2;
/// Blocks epoch allowed
const BLOCKS_PER_EPOCH: u64 = 5;

// A cap on the size of the future_sink
const SINK_CAP: usize = 1000;

/// Enum for pubsub channel that defines message type variant and data contained in message type.
#[derive(Clone, Debug)]
pub enum HeadChange {
    Current(Arc<Tipset>),
    Apply(Arc<Tipset>),
    Revert(Arc<Tipset>),
}

/// Generic implementation of the datastore trait and structures
pub struct ChainStore<DB> {
    publisher: Publisher<HeadChange>,

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
            publisher: Publisher::new(SINK_CAP),
            tip_index: TipIndex::new(),
            heaviest,
        }
    }

    /// Sets heaviest tipset within ChainStore and store its tipset cids under HEAD_KEY
    pub async fn set_heaviest_tipset(&mut self, ts: Arc<Tipset>) -> Result<(), Error> {
        self.db.write(HEAD_KEY, ts.key().marshal_cbor()?)?;
        self.heaviest = Some(ts.clone());
        self.publisher.publish(HeadChange::Current(ts)).await;
        Ok(())
    }

    // subscribing returns a future sink that we can essentially iterate over using future streams
    pub fn subscribe(&mut self) -> Subscriber<HeadChange> {
        self.publisher.subscribe()
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
    pub fn set_genesis(&self, header: BlockHeader) -> Result<Cid, Error> {
        set_genesis(self.blockstore(), header)
    }

    /// Writes tipset block headers to data store and updates heaviest tipset
    pub async fn put_tipset(&mut self, ts: &Tipset) -> Result<(), Error> {
        persist_objects(self.blockstore(), ts.blocks())?;
        // TODO determine if expanded tipset is required; see https://github.com/filecoin-project/lotus/blob/testnet/3/chain/store/store.go#L236
        self.update_heaviest(ts).await?;
        Ok(())
    }

    /// Writes encoded message data to blockstore
    pub fn put_messages<T: Cbor>(&self, msgs: &[T]) -> Result<(), Error> {
        persist_objects(self.blockstore(), msgs)
    }

    /// Loads heaviest tipset from datastore and sets as heaviest in chainstore
    pub async fn load_heaviest_tipset(&mut self) -> Result<(), Error> {
        let heaviest_ts = get_heaviest_tipset(self.blockstore())?.ok_or_else(|| {
            warn!("No previous chain state found");
            Error::Other("No chain state found".to_owned())
        })?;

        // set as heaviest tipset
        let heaviest_ts = Arc::new(heaviest_ts);
        self.heaviest = Some(heaviest_ts.clone());
        self.publisher
            .publish(HeadChange::Current(heaviest_ts))
            .await;
        Ok(())
    }

    /// Returns genesis blockheader from blockstore
    pub fn genesis(&self) -> Result<Option<BlockHeader>, Error> {
        genesis(self.blockstore())
    }

    /// Finds the latest beacon entry given a tipset up to 20 blocks behind
    pub fn latest_beacon_entry(&self, ts: &Tipset) -> Result<BeaconEntry, Error> {
        let mut cur = ts.clone();
        for _ in 1..20 {
            let cbe = ts.blocks()[0].beacon_entries();
            if let Some(entry) = cbe.last() {
                return Ok(entry.clone());
            }
            if cur.epoch() == 0 {
                return Err(Error::Other(
                    "made it back to genesis block without finding beacon entry".to_owned(),
                ));
            }
            cur = self.tipset_from_keys(cur.parents())?;
        }
        Err(Error::Other(
            "Found no beacon entries in the 20 blocks prior to the given tipset".to_owned(),
        ))
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

    /// Constructs and returns a full tipset if messages from storage exists
    pub fn fill_tipsets(&self, ts: Tipset) -> Result<FullTipset, Error> {
        let mut blocks: Vec<Block> = Vec::with_capacity(ts.blocks().len());

        for header in ts.into_blocks() {
            let (bls_messages, secp_messages) = block_messages(self.blockstore(), &header)?;
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
    async fn update_heaviest(&mut self, ts: &Tipset) -> Result<(), Error> {
        match &self.heaviest {
            Some(heaviest) => {
                let new_weight = weight(self.blockstore(), ts)?;
                let curr_weight = weight(self.blockstore(), &heaviest)?;
                if new_weight > curr_weight {
                    // TODO potentially need to deal with re-orgs here
                    info!("New heaviest tipset");
                    self.set_heaviest_tipset(Arc::new(ts.clone())).await?;
                }
            }
            None => {
                info!("set heaviest tipset");
                self.set_heaviest_tipset(Arc::new(ts.clone())).await?;
            }
        }
        Ok(())
    }
}

/// Returns messages for a given tipset from db
pub fn unsigned_messages_for_tipset<DB>(db: &DB, h: &Tipset) -> Result<Vec<UnsignedMessage>, Error>
where
    DB: BlockStore,
{
    let mut umsg: Vec<UnsignedMessage> = Vec::new();
    for bh in h.blocks().iter() {
        let (mut bh_umsg, bh_msg) = block_messages(db, bh)?;
        umsg.append(&mut bh_umsg);
        umsg.extend(bh_msg.into_iter().map(|msg| msg.into_message()));
    }
    Ok(umsg)
}

/// Returns a Tuple of bls messages of type UnsignedMessage and secp messages
/// of type SignedMessage
pub fn block_messages<DB>(
    db: &DB,
    bh: &BlockHeader,
) -> Result<(Vec<UnsignedMessage>, Vec<SignedMessage>), Error>
where
    DB: BlockStore,
{
    let (bls_cids, secpk_cids) = read_msg_cids(db, bh.messages())?;

    let bls_msgs: Vec<UnsignedMessage> = messages_from_cids(db, &bls_cids)?;
    let secp_msgs: Vec<SignedMessage> = messages_from_cids(db, &secpk_cids)?;

    Ok((bls_msgs, secp_msgs))
}

/// Returns a tuple of UnsignedMessage and SignedMessages from their Cid
pub fn block_messages_from_cids<DB>(
    db: &DB,
    bls_cids: &[Cid],
    secp_cids: &[Cid],
) -> Result<(Vec<UnsignedMessage>, Vec<SignedMessage>), Error>
where
    DB: BlockStore,
{
    let bls_msgs: Vec<UnsignedMessage> = messages_from_cids(db, bls_cids)?;
    let secp_msgs: Vec<SignedMessage> = messages_from_cids(db, secp_cids)?;

    Ok((bls_msgs, secp_msgs))
}

/// Returns a tuple of cids for both Unsigned and Signed messages
pub fn read_msg_cids<DB>(db: &DB, msg_cid: &Cid) -> Result<(Vec<Cid>, Vec<Cid>), Error>
where
    DB: BlockStore,
{
    if let Some(roots) = db
        .get::<TxMeta>(msg_cid)
        .map_err(|e| Error::Other(e.to_string()))?
    {
        let bls_cids = read_amt_cids(db, &roots.bls_message_root)?;
        let secpk_cids = read_amt_cids(db, &roots.secp_message_root)?;
        Ok((bls_cids, secpk_cids))
    } else {
        Err(Error::UndefinedKey("no msgs with that key".to_string()))
    }
}

fn set_genesis<DB>(db: &DB, header: BlockHeader) -> Result<Cid, Error>
where
    DB: BlockStore,
{
    db.write(GENESIS_KEY, header.marshal_cbor()?)?;
    Ok(db
        .put(&header, Blake2b256)
        .map_err(|e| Error::Other(e.to_string()))?)
}

/// Persists slice of serializable objects to blockstore.
pub fn persist_objects<DB, C>(db: &DB, headers: &[C]) -> Result<(), Error>
where
    DB: BlockStore,
    C: Serialize,
{
    for chunk in headers.chunks(256) {
        db.bulk_put(chunk, Blake2b256)
            .map_err(|e| Error::Other(e.to_string()))?;
    }
    Ok(())
}

/// Gets 32 bytes of randomness for ChainRand paramaterized by the DomainSeparationTag, ChainEpoch, Entropy
pub fn get_randomness<DB: BlockStore>(
    db: &DB,
    blocks: &TipsetKeys,
    pers: DomainSeparationTag,
    round: ChainEpoch,
    entropy: &[u8],
) -> Result<[u8; 32], Box<dyn std::error::Error>> {
    let mut blks = blocks.clone();
    loop {
        let nts = tipset_from_keys(db, &blks)?;
        let mtb = nts.min_ticket_block();
        if nts.epoch() <= round || mtb.epoch() == 0 {
            return draw_randomness(mtb.ticket().vrfproof.as_bytes(), pers, round, entropy);
        }
        blks = mtb.parents().clone();
    }
}

/// Computes a pseudorandom 32 byte Vec
pub fn draw_randomness(
    rbase: &[u8],
    pers: DomainSeparationTag,
    round: ChainEpoch,
    entropy: &[u8],
) -> Result<[u8; 32], Box<dyn std::error::Error>> {
    let mut state = Params::new().hash_length(32).to_state();
    state.write_i64::<BigEndian>(pers as i64)?;
    let vrf_digest = blake2b_256(rbase);
    state.write_all(&vrf_digest)?;
    state.write_i64::<BigEndian>(round as i64)?;
    state.write_all(entropy)?;
    let mut ret = [0u8; 32];
    ret.clone_from_slice(state.finalize().as_bytes());
    Ok(ret)
}

/// Returns the heaviest tipset
pub fn get_heaviest_tipset<DB>(db: &DB) -> Result<Option<Tipset>, Error>
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
pub fn tipset_from_keys<DB>(db: &DB, tsk: &TipsetKeys) -> Result<Tipset, Error>
where
    DB: BlockStore,
{
    let block_headers: Vec<BlockHeader> = tsk
        .cids()
        .iter()
        .map(|c| {
            db.get(c)
                .map_err(|e| Error::Other(e.to_string()))?
                .ok_or_else(|| Error::NotFound("Key for header"))
        })
        .collect::<Result<_, Error>>()?;

    // construct new Tipset to return
    let ts = Tipset::new(block_headers)?;
    Ok(ts)
}
/// Returns the tipset behind `tsk` at a given `height`. If the given height
/// is a null round:
/// if `prev` is `true`, the tipset before the null round is returned.
/// If `prev` is `false`, the tipset following the null round is returned.
pub fn tipset_by_height<DB>(
    db: &DB,
    height: ChainEpoch,
    ts: Tipset,
    prev: bool,
) -> Result<Tipset, Error>
where
    DB: BlockStore,
{
    if height > ts.epoch() {
        return Err(Error::Other(
            "searching for tipset that has a height less than starting point".to_owned(),
        ));
    }
    if height == ts.epoch() {
        return Ok(ts);
    }
    // TODO: If ts.epoch()-h > Fork Length Threshold, it could be expensive to look up
    let mut ts_temp = ts;
    loop {
        let pts = tipset_from_keys(db, ts_temp.parents())?;
        if height > pts.epoch() {
            if prev {
                return Ok(pts);
            }
            return Ok(ts_temp);
        }
        if height == pts.epoch() {
            return Ok(pts);
        }
        ts_temp = pts;
    }
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

/// Returns the genesis block
pub fn genesis<DB>(db: &DB) -> Result<Option<BlockHeader>, Error>
where
    DB: BlockStore,
{
    Ok(match db.read(GENESIS_KEY)? {
        Some(bz) => Some(BlockHeader::unmarshal_cbor(&bz)?),
        None => None,
    })
}

/// Attempts to deserialize to unsigend message or signed message and then returns it at as a message trait object
pub fn get_chain_message<DB>(db: &DB, key: &Cid) -> Result<ChainMessage, Error>
where
    DB: BlockStore,
{
    let value = db
        .read(key.key())?
        .ok_or_else(|| Error::UndefinedKey(key.to_string()))?;
    if let Ok(message) = from_slice::<UnsignedMessage>(&value) {
        Ok(ChainMessage::Unsigned(message))
    } else {
        let signed_message: SignedMessage = from_slice(&value)?;
        Ok(ChainMessage::Signed(signed_message))
    }
}

/// given a tipset this function will return all messages
pub fn messages_for_tipset<DB>(db: &DB, ts: &Tipset) -> Result<Vec<ChainMessage>, Error>
where
    DB: BlockStore,
{
    let mut applied: HashMap<Address, u64> = HashMap::new();
    let mut balances: HashMap<Address, BigInt> = HashMap::new();
    let state = StateTree::new_from_root(db, ts.parent_state())?;

    // message to get all messages for block_header into a single iterator
    let mut get_message_for_block_header = |b: &BlockHeader| -> Result<Vec<ChainMessage>, Error> {
        let (unsigned, signed) = block_messages(db, b)?;
        let mut messages = Vec::with_capacity(unsigned.len() + signed.len());
        let unsigned_box = unsigned.into_iter().map(ChainMessage::Unsigned);
        let signed_box = signed.into_iter().map(ChainMessage::Signed);

        for message in unsigned_box.chain(signed_box) {
            let from_address = message.from();
            if applied.contains_key(&from_address) {
                let actor_state = state
                    .get_actor(from_address)?
                    .ok_or_else(|| Error::Other("Actor state not found".to_string()))?;
                applied.insert(*from_address, actor_state.sequence);
                balances.insert(*from_address, actor_state.balance);
            }
            if let Some(seq) = applied.get_mut(from_address) {
                if *seq != message.sequence() {
                    continue;
                }
                *seq += 1;
            } else {
                continue;
            }
            if let Some(bal) = balances.get_mut(from_address) {
                if *bal < message.required_funds() {
                    continue;
                }
                *bal -= message.required_funds();
            } else {
                continue;
            }

            messages.push(message)
        }

        Ok(messages)
    };

    ts.blocks().iter().fold(Ok(Vec::new()), |vec, b| {
        let mut message_vec = vec?;
        let mut messages = get_message_for_block_header(b)?;
        message_vec.append(&mut messages);
        Ok(message_vec)
    })
}

/// Returns messages from key-value store
fn messages_from_cids<DB, T>(db: &DB, keys: &[Cid]) -> Result<Vec<T>, Error>
where
    DB: BlockStore,
    T: DeserializeOwned,
{
    keys.iter()
        .map(|k| {
            db.get(k)
                .map_err(|e| Error::Other(e.to_string()))?
                .ok_or_else(|| Error::UndefinedKey(k.to_string()))
        })
        .collect()
}

/// returns message receipt given block_header
pub fn get_parent_reciept<DB>(
    db: &DB,
    block_header: &BlockHeader,
    i: u64,
) -> Result<Option<MessageReceipt>, Error>
where
    DB: BlockStore,
{
    let amt = Amt::load(block_header.message_receipts(), db)?;
    let receipts = amt.get(i)?;
    Ok(receipts)
}

/// Returns the weight of provided tipset
fn weight<DB>(db: &DB, ts: &Tipset) -> Result<BigInt, String>
where
    DB: BlockStore,
{
    let mut tpow = BigInt::zero();
    let state = StateTree::new_from_root(db, ts.parent_state())?;
    if let Some(act) = state.get_actor(&*STORAGE_POWER_ACTOR_ADDR)? {
        if let Some(state) = db
            .get::<PowerState>(&act.state)
            .map_err(|e| e.to_string())?
        {
            tpow = state.total_quality_adj_power;
        }
    }
    let log2_p = if tpow > BigInt::zero() {
        BigInt::from(tpow.bits() - 1)
    } else {
        return Err(
            "All power in the net is gone. You network might be disconnected, or the net is dead!"
                .to_owned(),
        );
    };

    let out_add: BigInt = &log2_p << 8;
    let mut out = BigInt::from_biguint(Sign::Plus, ts.weight().to_owned()) + out_add;
    let e_weight = ((log2_p * BigInt::from(ts.blocks().len())) * BigInt::from(W_RATIO_NUM)) << 8;
    let value: BigInt = e_weight / (BigInt::from(BLOCKS_PER_EPOCH) * BigInt::from(W_RATIO_DEN));
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
