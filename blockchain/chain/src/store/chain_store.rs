// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{Error, TipIndex, TipsetMetadata};
use actor::{power::State as PowerState, STORAGE_POWER_ACTOR_ADDR};
use address::Address;
use async_std::sync::RwLock;
use async_std::task;
use beacon::{BeaconEntry, IGNORE_DRAND_VAR};
use blake2b_simd::Params;
use blocks::{Block, BlockHeader, FullTipset, Tipset, TipsetKeys, TxMeta};
use byteorder::{BigEndian, WriteBytesExt};
use cid::multihash::Blake2b256;
use cid::Cid;
use clock::ChainEpoch;
use crypto::DomainSeparationTag;
use encoding::{blake2b_256, de::DeserializeOwned, from_slice, Cbor};
use flo_stream::{MessagePublisher, Publisher, Subscriber};
use futures::{future, StreamExt};
use ipld_amt::Amt;
use ipld_blockstore::BlockStore;
use log::{info, warn};
use message::{ChainMessage, Message, MessageReceipt, SignedMessage, UnsignedMessage};
use num_bigint::{BigInt, Integer};
use num_traits::Zero;
use serde::Serialize;
use state_tree::StateTree;
use std::collections::HashMap;
use std::io::Write;
use std::sync::Arc;
use types::WINNING_POST_SECTOR_SET_LOOKBACK;

const GENESIS_KEY: &str = "gen_block";
pub const HEAD_KEY: &str = "head";
const BLOCK_VAL_PREFIX: &[u8] = b"block_val/";

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

#[derive(Debug, Clone)]
pub struct IndexToHeadChange(pub usize, pub HeadChange);

#[derive(Clone)]
pub enum EventsPayload {
    TaskCancel(usize, ()),
    SubHeadChanges(IndexToHeadChange),
}

impl EventsPayload {
    pub fn sub_head_changes(&self) -> Option<&IndexToHeadChange> {
        match self {
            EventsPayload::SubHeadChanges(s) => Some(s),
            _ => None,
        }
    }

    pub fn task_cancel(&self) -> Option<(usize, ())> {
        match self {
            EventsPayload::TaskCancel(val, _) => Some((*val, ())),
            _ => None,
        }
    }
}

/// Stores chain data such as heaviest tipset and cached tipset info at each epoch.
/// This structure is threadsafe, and all caches are wrapped in a mutex to allow a consistent
/// `ChainStore` to be shared across tasks.
pub struct ChainStore<DB> {
    publisher: RwLock<Publisher<HeadChange>>,

    // key-value datastore
    pub db: Arc<DB>,

    // Tipset at the head of the best-known chain.
    heaviest: RwLock<Option<Arc<Tipset>>>,

    // tip_index tracks tipsets by epoch/parentset for use by expected consensus.
    tip_index: RwLock<TipIndex>,
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
            publisher: RwLock::new(Publisher::new(SINK_CAP)),
            tip_index: RwLock::new(TipIndex::new()),
            heaviest: RwLock::new(heaviest),
        }
    }

    /// Sets heaviest tipset within ChainStore and store its tipset cids under HEAD_KEY
    pub async fn set_heaviest_tipset(&self, ts: Arc<Tipset>) -> Result<(), Error> {
        self.db.write(HEAD_KEY, ts.key().marshal_cbor()?)?;
        *self.heaviest.write().await = Some(ts.clone());
        self.publisher
            .write()
            .await
            .publish(HeadChange::Current(ts))
            .await;
        Ok(())
    }

    // subscribing returns a future sink that we can essentially iterate over using future streams
    pub async fn subscribe(&self) -> Subscriber<HeadChange> {
        self.publisher.write().await.subscribe()
    }

    /// Sets tip_index tracker
    // TODO this is really broken, should not be setting the tipset metadata to a tipset with just
    // the one header.
    pub async fn set_tipset_tracker(&self, header: &BlockHeader) -> Result<(), Error> {
        let ts = Arc::new(Tipset::new(vec![header.clone()])?);
        let meta = TipsetMetadata {
            tipset_state_root: header.state_root().clone(),
            tipset_receipts_root: header.message_receipts().clone(),
            tipset: ts,
        };
        self.tip_index.write().await.put(&meta).await;
        Ok(())
    }

    /// Writes genesis to blockstore
    pub fn set_genesis(&self, header: &BlockHeader) -> Result<Cid, Error> {
        set_genesis(self.blockstore(), header)
    }

    /// Writes tipset block headers to data store and updates heaviest tipset
    pub async fn put_tipset(&self, ts: &Tipset) -> Result<(), Error> {
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
    pub async fn load_heaviest_tipset(&self) -> Result<(), Error> {
        let heaviest_ts = get_heaviest_tipset(self.blockstore())?.ok_or_else(|| {
            warn!("No previous chain state found");
            Error::Other("No chain state found".to_owned())
        })?;

        // set as heaviest tipset
        let heaviest_ts = Arc::new(heaviest_ts);
        *self.heaviest.write().await = Some(heaviest_ts.clone());
        self.publisher
            .write()
            .await
            .publish(HeadChange::Current(heaviest_ts))
            .await;
        Ok(())
    }

    /// Returns genesis blockheader from blockstore
    pub fn genesis(&self) -> Result<Option<BlockHeader>, Error> {
        genesis(self.blockstore())
    }

    /// Returns heaviest tipset from blockstore
    pub async fn heaviest_tipset(&self) -> Option<Arc<Tipset>> {
        self.heaviest.read().await.clone()
    }

    pub fn tip_index(&self) -> &RwLock<TipIndex> {
        &self.tip_index
    }

    pub fn publisher(&self) -> &RwLock<Publisher<HeadChange>> {
        &self.publisher
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
    pub fn fill_tipset(&self, ts: Tipset) -> Result<FullTipset, Tipset> {
        fill_tipset(self.blockstore(), ts)
    }

    /// Determines if provided tipset is heavier than existing known heaviest tipset
    async fn update_heaviest(&self, ts: &Tipset) -> Result<(), Error> {
        match self.heaviest.read().await.as_ref() {
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

    /// Checks store if block has already been validated. Key based on the block validation prefix.
    pub fn is_block_validated(&self, cid: &Cid) -> Result<bool, Error> {
        let key = block_validation_key(cid);

        Ok(self.db.exists(key)?)
    }

    /// Marks block as validated in the store. This is retrieved using the block validation prefix.
    pub fn mark_block_as_validated(&self, cid: &Cid) -> Result<(), Error> {
        let key = block_validation_key(cid);

        Ok(self.db.write(key, &[])?)
    }

    /// Gets lookback tipset for block validations.
    /// Returns `None` if the tipset is also the lookback tipset.
    pub fn get_lookback_tipset_for_round(
        &self,
        ts: &Tipset,
        round: ChainEpoch,
    ) -> Result<Option<Tipset>, Error> {
        let lbr = if round > WINNING_POST_SECTOR_SET_LOOKBACK {
            round - WINNING_POST_SECTOR_SET_LOOKBACK
        } else {
            0
        };

        if lbr > ts.epoch() {
            return Ok(None);
        }

        // TODO would be better to get tipset with ChainStore cache.
        tipset_by_height(self.blockstore(), lbr, ts, true)
    }
}

/// Helper to ensure consistent Cid -> db key translation.
fn block_validation_key(cid: &Cid) -> Vec<u8> {
    let mut key = Vec::new();
    key.extend_from_slice(BLOCK_VAL_PREFIX);
    key.extend(cid.to_bytes());
    key
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

/// Returns a vector of all chain messages, these messages contain all bls messages followed
/// by all secp messages.
// TODO try to group functionality with block_messages
pub fn chain_messages<DB>(db: &DB, bh: &BlockHeader) -> Result<Vec<ChainMessage>, Error>
where
    DB: BlockStore,
{
    let (bls_cids, secpk_cids) = read_msg_cids(db, bh.messages())?;

    let mut bls_msgs: Vec<ChainMessage> = messages_from_cids(db, &bls_cids)?;
    let mut secp_msgs: Vec<ChainMessage> = messages_from_cids(db, &secpk_cids)?;

    // Append the secp messages to the back of the messages vector.
    bls_msgs.append(&mut secp_msgs);

    Ok(bls_msgs)
}

/// Constructs and returns a full tipset if messages from storage exists - non self version
pub fn fill_tipset<DB>(db: &DB, ts: Tipset) -> Result<FullTipset, Tipset>
where
    DB: BlockStore,
{
    // Collect all messages before moving tipset.
    let messages: Vec<(Vec<_>, Vec<_>)> = match ts
        .blocks()
        .iter()
        .map(|h| block_messages(db, h))
        .collect::<Result<_, Error>>()
    {
        Ok(m) => m,
        Err(e) => {
            log::trace!("failed to fill tipset: {}", e);
            return Err(ts);
        }
    };

    // Zip messages with blocks
    let blocks = ts
        .into_blocks()
        .into_iter()
        .zip(messages)
        .map(|(header, (bls_messages, secp_messages))| Block {
            header,
            bls_messages,
            secp_messages,
        })
        .collect();

    // the given tipset has already been verified, so this cannot fail
    Ok(FullTipset::new(blocks).unwrap())
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

/// Sets the genesis key in the BlockStore. Be careful if using this outside of
/// the ChainStore as it will not update what the ChainStore thinks is the genesis
/// after the ChainStore has been created.
pub fn set_genesis<DB>(db: &DB, header: &BlockHeader) -> Result<Cid, Error>
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

/// Finds the latest beacon entry given a tipset up to 20 tipsets behind
pub fn latest_beacon_entry<DB>(db: &DB, ts: &Tipset) -> Result<BeaconEntry, Error>
where
    DB: BlockStore,
{
    let check_for_beacon_entry = |ts: &Tipset| {
        let cbe = ts.min_ticket_block().beacon_entries();
        if let Some(entry) = cbe.last() {
            return Ok(Some(entry.clone()));
        }
        if ts.epoch() == 0 {
            return Err(Error::Other(
                "made it back to genesis block without finding beacon entry".to_owned(),
            ));
        }
        Ok(None)
    };

    if let Some(entry) = check_for_beacon_entry(ts)? {
        return Ok(entry);
    }
    let mut cur = tipset_from_keys(db, ts.parents())?;
    for i in 1..20 {
        if i != 1 {
            cur = tipset_from_keys(db, cur.parents())?;
        }
        if let Some(entry) = check_for_beacon_entry(&cur)? {
            return Ok(entry);
        }
    }

    if std::env::var(IGNORE_DRAND_VAR) == Ok("1".to_owned()) {
        return Ok(BeaconEntry::new(
            0,
            vec![9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9],
        ));
    }

    Err(Error::Other(
        "Found no beacon entries in the 20 latest tipsets".to_owned(),
    ))
}

/// Gets 32 bytes of randomness for ChainRand paramaterized by the DomainSeparationTag, ChainEpoch,
/// Entropy from the ticket chain.
pub fn get_chain_randomness<DB: BlockStore>(
    db: &DB,
    blocks: &TipsetKeys,
    pers: DomainSeparationTag,
    round: ChainEpoch,
    entropy: &[u8],
) -> Result<[u8; 32], Box<dyn std::error::Error>> {
    let ts = tipset_from_keys(db, blocks)?;

    if round > ts.epoch() {
        return Err("cannot draw randomness from the future".into());
    }

    let search_height = if round < 0 { 0 } else { round };

    let rand_ts = tipset_by_height(db, search_height, &ts, true)?.unwrap_or(ts);

    draw_randomness(
        rand_ts
            .min_ticket()
            .ok_or("No ticket exists for block")?
            .vrfproof
            .as_bytes(),
        pers,
        round,
        entropy,
    )
}

/// Gets 32 bytes of randomness for ChainRand paramaterized by the DomainSeparationTag, ChainEpoch,
/// Entropy from the latest beacon entry.
pub fn get_beacon_randomness<DB: BlockStore>(
    db: &DB,
    blocks: &TipsetKeys,
    pers: DomainSeparationTag,
    round: ChainEpoch,
    entropy: &[u8],
) -> Result<[u8; 32], Box<dyn std::error::Error>> {
    let ts = tipset_from_keys(db, blocks)?;

    if round > ts.epoch() {
        return Err("cannot draw randomness from the future".into());
    }

    let search_height = if round < 0 { 0 } else { round };

    let rand_ts = tipset_by_height(db, search_height, &ts, true)?.unwrap_or(ts);

    let be = latest_beacon_entry(db, &rand_ts)?;

    draw_randomness(be.data(), pers, round, entropy)
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
/// Returns the tipset behind `tsk` at a given `height`.
/// If the given height is a null round:
/// - If `prev` is `true`, the tipset before the null round is returned.
/// - If `prev` is `false`, the tipset following the null round is returned.
///
/// Returns `None` if the tipset provided was the tipset at the given height.
pub fn tipset_by_height<DB>(
    db: &DB,
    height: ChainEpoch,
    ts: &Tipset,
    prev: bool,
) -> Result<Option<Tipset>, Error>
where
    DB: BlockStore,
{
    if height > ts.epoch() {
        return Err(Error::Other(
            "searching for tipset that has a height less than starting point".to_owned(),
        ));
    }
    if height == ts.epoch() {
        return Ok(None);
    }
    // TODO: If ts.epoch()-h > Fork Length Threshold, it could be expensive to look up
    let mut ts_temp: Option<Tipset> = None;
    loop {
        let pts = if let Some(temp) = &ts_temp {
            tipset_from_keys(db, temp.parents())?
        } else {
            tipset_from_keys(db, ts.parents())?
        };
        if height > pts.epoch() {
            if prev {
                return Ok(Some(pts));
            }
            return Ok(ts_temp);
        }
        if height == pts.epoch() {
            return Ok(Some(pts));
        }
        ts_temp = Some(pts);
    }
}

/// Returns a vector of cids from provided root cid
fn read_amt_cids<DB>(db: &DB, root: &Cid) -> Result<Vec<Cid>, Error>
where
    DB: BlockStore,
{
    let amt = Amt::<Cid, _>::load(root, db)?;

    let mut cids = Vec::new();
    for i in 0..amt.count() {
        if let Some(c) = amt.get(i)? {
            cids.push(c.clone());
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
    let state =
        StateTree::new_from_root(db, ts.parent_state()).map_err(|e| Error::Other(e.to_string()))?;

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
                    .get_actor(from_address)
                    .map_err(|e| Error::Other(e.to_string()))?
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
    Ok(receipts.cloned())
}

/// Returns the weight of provided tipset
pub fn weight<DB>(db: &DB, ts: &Tipset) -> Result<BigInt, String>
where
    DB: BlockStore,
{
    let mut tpow = BigInt::zero();
    let state = StateTree::new_from_root(db, ts.parent_state()).map_err(|e| e.to_string())?;
    if let Some(act) = state
        .get_actor(&*STORAGE_POWER_ACTOR_ADDR)
        .map_err(|e| e.to_string())?
    {
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

    let mut total_j = 0;
    for b in ts.blocks() {
        total_j += b
            .election_proof()
            .as_ref()
            .ok_or("Block contained no election proof when calculating weight")?
            .win_count;
    }

    let mut out = ts.weight().to_owned();
    out += &log2_p << 8;
    let mut e_weight: BigInt = log2_p * W_RATIO_NUM;
    e_weight <<= 8;
    e_weight *= total_j;
    e_weight = e_weight.div_floor(&(BigInt::from(BLOCKS_PER_EPOCH * W_RATIO_DEN)));
    out += &e_weight;
    Ok(out)
}

pub async fn sub_head_changes(
    mut subscribed_head_change: Subscriber<HeadChange>,
    heaviest_tipset: &Option<Arc<Tipset>>,
    current_index: usize,
    events_pubsub: Arc<RwLock<Publisher<EventsPayload>>>,
) -> Result<usize, Error> {
    let head = heaviest_tipset
        .as_ref()
        .ok_or_else(|| Error::Other("Could not get heaviest tipset".to_string()))?;

    (*events_pubsub.write().await)
        .publish(EventsPayload::SubHeadChanges(IndexToHeadChange(
            current_index,
            HeadChange::Current(head.clone()),
        )))
        .await;
    let subhead_sender = events_pubsub.clone();
    let handle = task::spawn(async move {
        while let Some(change) = subscribed_head_change.next().await {
            let index_to_head_change = IndexToHeadChange(current_index, change);
            subhead_sender
                .write()
                .await
                .publish(EventsPayload::SubHeadChanges(index_to_head_change))
                .await;
        }
    });
    let cancel_sender = events_pubsub.write().await.subscribe();
    task::spawn(async move {
        if let Some(EventsPayload::TaskCancel(_, ())) = cancel_sender
            .filter(|s| {
                future::ready(
                    s.task_cancel()
                        .map(|s| s.0 == current_index)
                        .unwrap_or_default(),
                )
            })
            .next()
            .await
        {
            handle.cancel().await;
        }
    });
    Ok(current_index)
}

#[cfg(feature = "json")]
pub mod headchange_json {
    use super::*;
    use blocks::tipset_json::TipsetJsonRef;
    use serde::Serialize;

    #[derive(Serialize)]
    #[serde(rename_all = "lowercase")]
    #[serde(tag = "type", content = "val")]
    pub enum HeadChangeJson<'a> {
        Current(TipsetJsonRef<'a>),
        Apply(TipsetJsonRef<'a>),
        Revert(TipsetJsonRef<'a>),
    }

    impl<'a> From<&'a HeadChange> for HeadChangeJson<'a> {
        fn from(wrapper: &'a HeadChange) -> Self {
            match wrapper {
                HeadChange::Current(tipset) => HeadChangeJson::Current(TipsetJsonRef(&tipset)),
                HeadChange::Apply(tipset) => HeadChangeJson::Apply(TipsetJsonRef(&tipset)),
                HeadChange::Revert(tipset) => HeadChangeJson::Revert(TipsetJsonRef(&tipset)),
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use address::Address;
    use cid::multihash::{Identity, Sha2_256};

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
        cs.set_genesis(&gen_block).unwrap();
        assert_eq!(cs.genesis().unwrap(), Some(gen_block));
    }

    #[test]
    fn block_validation_cache_basic() {
        let db = db::MemoryDB::default();

        let cs = ChainStore::new(Arc::new(db));

        let cid = Cid::new_from_cbor(&[1, 2, 3], Sha2_256);
        assert_eq!(cs.is_block_validated(&cid).unwrap(), false);

        cs.mark_block_as_validated(&cid).unwrap();
        assert_eq!(cs.is_block_validated(&cid).unwrap(), true);
    }
}
