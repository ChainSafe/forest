// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{Error, TipIndex};
use actor::{power::State as PowerState, STORAGE_POWER_ACTOR_ADDR};
use address::Address;
use async_std::sync::RwLock;
use async_std::task;
use beacon::{BeaconEntry, IGNORE_DRAND_VAR};
use blake2b_simd::Params;
use blocks::{Block, BlockHeader, FullTipset, Tipset, TipsetKeys, TxMeta};
use byteorder::{BigEndian, WriteBytesExt};
use cid::Cid;
use cid::Code::Blake2b256;
use clock::ChainEpoch;
use crypto::DomainSeparationTag;
use encoding::{blake2b_256, de::DeserializeOwned, from_slice, Cbor};
use flo_stream::{MessagePublisher, Publisher, Subscriber};
use futures::{future, StreamExt};
use interpreter::BlockMessages;
use ipld_amt::Amt;
use ipld_blockstore::BlockStore;
use log::{info, warn};
use lru::LruCache;
use message::{ChainMessage, Message, MessageReceipt, SignedMessage, UnsignedMessage};
use num_bigint::{BigInt, Integer};
use num_traits::Zero;
use rayon::prelude::*;
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

const DEFAULT_TIPSET_CACHE_SIZE: usize = 8192;

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
    /// Publisher for head change events
    publisher: RwLock<Publisher<HeadChange>>,

    /// key-value datastore.
    pub db: Arc<DB>,

    /// Tipset at the head of the best-known chain.
    heaviest: RwLock<Option<Arc<Tipset>>>,

    ts_cache: RwLock<LruCache<TipsetKeys, Arc<Tipset>>>,

    /// tip_index tracks tipsets by epoch/parentset for use by expected consensus.
    tip_index: TipIndex,
}

impl<DB> ChainStore<DB>
where
    DB: BlockStore + Send + Sync + 'static,
{
    pub fn new(db: Arc<DB>) -> Self {
        let cs = Self {
            db,
            publisher: RwLock::new(Publisher::new(SINK_CAP)),
            tip_index: TipIndex::default(),
            ts_cache: RwLock::new(LruCache::new(DEFAULT_TIPSET_CACHE_SIZE)),
            heaviest: Default::default(),
        };

        // Result intentionally ignored, doesn't matter if heaviest doesn't exist in store yet
        let _ = task::block_on(cs.load_heaviest_tipset());

        cs
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
    // TODO handle managing tipset in tracker
    pub async fn set_tipset_tracker(&self, _: &BlockHeader) -> Result<(), Error> {
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

    /// Loads heaviest tipset from datastore and sets as heaviest in chainstore
    pub async fn load_heaviest_tipset(&self) -> Result<(), Error> {
        let heaviest_ts = match self.db.read(HEAD_KEY)? {
            Some(bz) => {
                let keys: Vec<Cid> = from_slice(&bz)?;
                self.tipset_from_keys(&TipsetKeys::new(keys)).await?
            }
            None => {
                warn!("No previous chain state found");
                return Err(Error::Other("No chain state found".to_owned()));
            }
        };

        // set as heaviest tipset
        let heaviest_ts = heaviest_ts;
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

    pub fn tip_index(&self) -> &TipIndex {
        &self.tip_index
    }

    pub fn publisher(&self) -> &RwLock<Publisher<HeadChange>> {
        &self.publisher
    }

    /// Returns key-value store instance.
    pub fn blockstore(&self) -> &DB {
        &self.db
    }

    /// Clones blockstore `Arc`.
    pub fn blockstore_cloned(&self) -> Arc<DB> {
        self.db.clone()
    }

    /// Returns Tipset from key-value store from provided cids
    pub async fn tipset_from_keys(&self, tsk: &TipsetKeys) -> Result<Arc<Tipset>, Error> {
        if let Some(ts) = self.ts_cache.write().await.get(tsk) {
            return Ok(ts.clone());
        }

        let block_headers: Vec<BlockHeader> = tsk
            .cids()
            .par_iter()
            .map(|c| {
                self.db
                    .get(c)
                    .map_err(|e| Error::Other(e.to_string()))?
                    .ok_or_else(|| Error::NotFound("Key for header"))
            })
            .collect::<Result<_, Error>>()?;

        // construct new Tipset to return
        let ts = Arc::new(Tipset::new(block_headers)?);
        self.ts_cache.write().await.put(tsk.clone(), ts.clone());
        Ok(ts)
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
    pub async fn get_lookback_tipset_for_round(
        &self,
        ts: &Tipset,
        round: ChainEpoch,
    ) -> Result<Option<Arc<Tipset>>, Error> {
        let lbr = if round > WINNING_POST_SECTOR_SET_LOOKBACK {
            round - WINNING_POST_SECTOR_SET_LOOKBACK
        } else {
            0
        };

        if lbr > ts.epoch() {
            return Ok(None);
        }

        // TODO would be better to get tipset with ChainStore cache.
        self.tipset_by_height(lbr, ts, true).await
    }

    /// Returns the tipset behind `tsk` at a given `height`.
    /// If the given height is a null round:
    /// - If `prev` is `true`, the tipset before the null round is returned.
    /// - If `prev` is `false`, the tipset following the null round is returned.
    ///
    /// Returns `None` if the tipset provided was the tipset at the given height.
    pub async fn tipset_by_height(
        &self,
        height: ChainEpoch,
        ts: &Tipset,
        prev: bool,
    ) -> Result<Option<Arc<Tipset>>, Error> {
        if height > ts.epoch() {
            return Err(Error::Other(
                "searching for tipset that has a height less than starting point".to_owned(),
            ));
        }
        if height == ts.epoch() {
            return Ok(None);
        }
        let mut ts_temp: Option<Arc<Tipset>> = None;
        loop {
            let pts = if let Some(temp) = &ts_temp {
                self.tipset_from_keys(temp.parents()).await?
            } else {
                self.tipset_from_keys(ts.parents()).await?
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

    /// Gets 32 bytes of randomness for ChainRand paramaterized by the DomainSeparationTag, ChainEpoch,
    /// Entropy from the ticket chain.
    pub async fn get_chain_randomness(
        &self,
        blocks: &TipsetKeys,
        pers: DomainSeparationTag,
        round: ChainEpoch,
        entropy: &[u8],
    ) -> Result<[u8; 32], Box<dyn std::error::Error>> {
        let ts = self.tipset_from_keys(blocks).await?;

        if round > ts.epoch() {
            return Err("cannot draw randomness from the future".into());
        }

        let search_height = if round < 0 { 0 } else { round };

        let rand_ts = self
            .tipset_by_height(search_height, &ts, true)
            .await?
            .unwrap_or(ts);

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
    pub async fn get_beacon_randomness(
        &self,
        blocks: &TipsetKeys,
        pers: DomainSeparationTag,
        round: ChainEpoch,
        entropy: &[u8],
    ) -> Result<[u8; 32], Box<dyn std::error::Error>> {
        let ts = self.tipset_from_keys(blocks).await?;

        if round > ts.epoch() {
            return Err("cannot draw randomness from the future".into());
        }

        let search_height = if round < 0 { 0 } else { round };

        let rand_ts = self
            .tipset_by_height(search_height, &ts, true)
            .await?
            .unwrap_or(ts);

        let be = self.latest_beacon_entry(&rand_ts).await?;

        draw_randomness(be.data(), pers, round, entropy)
    }

    /// Finds the latest beacon entry given a tipset up to 20 tipsets behind
    pub async fn latest_beacon_entry(&self, ts: &Tipset) -> Result<BeaconEntry, Error> {
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
        let mut cur = self.tipset_from_keys(ts.parents()).await?;
        for i in 1..20 {
            if i != 1 {
                cur = self.tipset_from_keys(cur.parents()).await?;
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

    /// Constructs and returns a full tipset if messages from storage exists - non self version
    pub fn fill_tipset(&self, ts: Arc<Tipset>) -> Result<FullTipset, Arc<Tipset>>
    where
        DB: BlockStore,
    {
        // Collect all messages before moving tipset.
        let messages: Vec<(Vec<_>, Vec<_>)> = match ts
            .blocks()
            .iter()
            .map(|h| block_messages(self.blockstore(), h))
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
            .blocks()
            .iter()
            .cloned()
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

    /// Retrieves block messages to be passed through the VM.
    pub fn block_msgs_for_tipset(&self, ts: &Tipset) -> Result<Vec<BlockMessages>, Error> {
        let mut applied = HashMap::new();
        let mut select_msg = |m: ChainMessage| -> Option<ChainMessage> {
            // The first match for a sender is guaranteed to have correct nonce
            // the block isn't valid otherwise.
            let entry = applied.entry(*m.from()).or_insert_with(|| m.sequence());

            if *entry != m.sequence() {
                return None;
            }

            *entry += 1;
            Some(m)
        };

        ts.blocks()
            .iter()
            .map(|b| {
                let (usm, sm) = block_messages(self.blockstore(), b)?;

                let mut messages = Vec::with_capacity(usm.len() + sm.len());
                messages.extend(
                    usm.into_iter()
                        .filter_map(|m| select_msg(ChainMessage::Unsigned(m))),
                );
                messages.extend(
                    sm.into_iter()
                        .filter_map(|m| select_msg(ChainMessage::Signed(m))),
                );

                Ok(BlockMessages {
                    miner: *b.miner_address(),
                    messages,
                    win_count: b
                        .election_proof()
                        .as_ref()
                        .map(|e| e.win_count)
                        .unwrap_or_default(),
                })
            })
            .collect()
    }

    /// Retrieves ordered valid messages from a `Tipset`. This will only include messages that will
    /// be passed through the VM.
    pub fn messages_for_tipset(&self, ts: &Tipset) -> Result<Vec<ChainMessage>, Error> {
        let bmsgs = self.block_msgs_for_tipset(ts)?;
        Ok(bmsgs.into_iter().map(|bm| bm.messages).flatten().collect())
    }
}

/// Helper to ensure consistent Cid -> db key translation.
fn block_validation_key(cid: &Cid) -> Vec<u8> {
    let mut key = Vec::new();
    key.extend_from_slice(BLOCK_VAL_PREFIX);
    key.extend(cid.to_bytes());
    key
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
// TODO cache these recent meta roots
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

/// Returns a vector of cids from provided root cid
fn read_amt_cids<DB>(db: &DB, root: &Cid) -> Result<Vec<Cid>, Error>
where
    DB: BlockStore,
{
    let amt = Amt::<Cid, _>::load(root, db)?;

    let mut cids = Vec::new();
    for i in 0..amt.count() {
        if let Some(c) = amt.get(i)? {
            cids.push(*c);
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
    db.get(key)
        .map_err(|e| Error::Other(e.to_string()))?
        .ok_or_else(|| Error::UndefinedKey(key.to_string()))
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
    use cid::Code::{Blake2b256, Identity};

    #[test]
    fn genesis_test() {
        let db = db::MemoryDB::default();

        let cs = ChainStore::new(Arc::new(db));
        let gen_block = BlockHeader::builder()
            .epoch(1)
            .weight((2 as u32).into())
            .messages(cid::new_from_cbor(&[], Identity))
            .message_receipts(cid::new_from_cbor(&[], Identity))
            .state_root(cid::new_from_cbor(&[], Identity))
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

        let cid = cid::new_from_cbor(&[1, 2, 3], Blake2b256);
        assert_eq!(cs.is_block_validated(&cid).unwrap(), false);

        cs.mark_block_as_validated(&cid).unwrap();
        assert_eq!(cs.is_block_validated(&cid).unwrap(), true);
    }
}
