// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{index::ChainIndex, tipset_tracker::TipsetTracker, Error};
use actor::{miner, power};
use address::Address;
use async_std::channel::{self, bounded, Receiver};
use async_std::sync::RwLock;
use async_std::task;
use beacon::{BeaconEntry, IGNORE_DRAND_VAR};
use blake2b_simd::Params;
use blocks::{Block, BlockHeader, FullTipset, Tipset, TipsetKeys, TxMeta};
use byteorder::{BigEndian, WriteBytesExt};
use cid::Cid;
use cid::Code::Blake2b256;
use clock::ChainEpoch;
use crossbeam::atomic::AtomicCell;
use crypto::DomainSeparationTag;
use encoding::{blake2b_256, de::DeserializeOwned, from_slice, Cbor};
use forest_car::CarHeader;
use forest_ipld::recurse_links;
use futures::AsyncWrite;
use interpreter::BlockMessages;
use ipld_amt::Amt;
use ipld_blockstore::BlockStore;
use lockfree::map::Map as LockfreeMap;
use log::{debug, info, trace, warn};
use lru::LruCache;
use message::{ChainMessage, Message, MessageReceipt, SignedMessage, UnsignedMessage};
use num_bigint::{BigInt, Integer};
use num_traits::Zero;
use serde::Serialize;
use state_tree::StateTree;
use std::error::Error as StdError;
use std::io::Write;
use std::sync::Arc;
use std::{
    collections::{HashMap, HashSet, VecDeque},
    time::SystemTime,
};
use tokio::sync::broadcast::{self, error::RecvError, Sender as Publisher};

const GENESIS_KEY: &str = "gen_block";
const HEAD_KEY: &str = "head";
const BLOCK_VAL_PREFIX: &[u8] = b"block_val/";

// constants for Weight calculation
/// The ratio of weight contributed by short-term vs long-term factors in a given round
const W_RATIO_NUM: u64 = 1;
const W_RATIO_DEN: u64 = 2;
/// Blocks epoch allowed
const BLOCKS_PER_EPOCH: u64 = 5;

// A cap on the size of the future_sink
const SINK_CAP: usize = 200;

const DEFAULT_TIPSET_CACHE_SIZE: usize = 8192;

/// Enum for pubsub channel that defines message type variant and data contained in message type.
#[derive(Clone, Debug)]
pub enum HeadChange {
    Current(Arc<Tipset>),
    Apply(Arc<Tipset>),
    Revert(Arc<Tipset>),
}

/// Stores chain data such as heaviest tipset and cached tipset info at each epoch.
/// This structure is threadsafe, and all caches are wrapped in a mutex to allow a consistent
/// `ChainStore` to be shared across tasks.
pub struct ChainStore<DB> {
    /// Publisher for head change events
    publisher: Publisher<HeadChange>,

    /// Tracks head change subscription channels
    subscriptions: Arc<LockfreeMap<i64, Option<Receiver<HeadChange>>>>,

    /// Keeps track of how many subscriptions there are in order to auto-increment the subscription ID
    subscriptions_count: AtomicCell<usize>,

    /// key-value datastore.
    pub db: Arc<DB>,

    /// Tipset at the head of the best-known chain.
    heaviest: RwLock<Option<Arc<Tipset>>>,

    /// Caches loaded tipsets for fast retrieval.
    ts_cache: Arc<TipsetCache>,

    /// Used as a cache for tipset lookbacks.
    chain_index: ChainIndex<DB>,

    /// Tracks blocks for the purpose of forming tipsets.
    tipset_tracker: TipsetTracker<DB>,
}

impl<DB> ChainStore<DB>
where
    DB: BlockStore + Send + Sync + 'static,
{
    pub fn new(db: Arc<DB>) -> Self {
        let (publisher, _) = broadcast::channel(SINK_CAP);
        let ts_cache = Arc::new(RwLock::new(LruCache::new(DEFAULT_TIPSET_CACHE_SIZE)));
        let cs = Self {
            publisher,
            subscriptions: Default::default(),
            subscriptions_count: Default::default(),
            chain_index: ChainIndex::new(ts_cache.clone(), db.clone()),
            tipset_tracker: TipsetTracker::new(db.clone()),
            db,
            ts_cache,
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
        if self.publisher.send(HeadChange::Apply(ts)).is_err() {
            debug!("did not publish head change, no active receivers");
        }
        Ok(())
    }

    /// Writes genesis to blockstore.
    pub fn set_genesis(&self, header: &BlockHeader) -> Result<Cid, Error> {
        set_genesis(self.blockstore(), header)
    }

    /// Adds a [BlockHeader] to the tipset tracker, which tracks valid headers.
    pub async fn add_to_tipset_tracker(&self, header: &BlockHeader) {
        self.tipset_tracker.add(header).await;
    }

    /// Writes tipset block headers to data store and updates heaviest tipset with other
    /// compatible tracked headers.
    pub async fn put_tipset(&self, ts: &Tipset) -> Result<(), Error> {
        // TODO: we could add the blocks of `ts` to the tipset tracker from here,
        // making `add_to_tipset_tracker` redundant and decreasing the number of
        // blockstore reads
        persist_objects(self.blockstore(), ts.blocks())?;

        // Expand tipset to include other compatible blocks at the epoch.
        let expanded = self.expand_tipset(ts.min_ticket_block().clone()).await?;
        self.update_heaviest(Arc::new(expanded)).await?;
        Ok(())
    }

    /// Expands tipset to tipset with all other headers in the same epoch using the tipset tracker.
    async fn expand_tipset(&self, header: BlockHeader) -> Result<Tipset, Error> {
        self.tipset_tracker.expand(header).await
    }

    /// Loads heaviest tipset from datastore and sets as heaviest in chainstore.
    async fn load_heaviest_tipset(&self) -> Result<(), Error> {
        let heaviest_ts = match self.db.read(HEAD_KEY)? {
            Some(bz) => self.tipset_from_keys(&from_slice(&bz)?).await?,
            None => {
                warn!("No previous chain state found");
                return Err(Error::Other("No chain state found".to_owned()));
            }
        };

        // set as heaviest tipset
        *self.heaviest.write().await = Some(heaviest_ts);
        Ok(())
    }

    /// Returns genesis [BlockHeader] from the store based on a static key.
    pub fn genesis(&self) -> Result<Option<BlockHeader>, Error> {
        genesis(self.blockstore())
    }

    /// Returns the currently tracked heaviest tipset.
    pub async fn heaviest_tipset(&self) -> Option<Arc<Tipset>> {
        // TODO: Figure out how to remove optional and return something everytime.
        self.heaviest.read().await.clone()
    }

    /// Returns a reference to the publisher of head changes.
    pub fn publisher(&self) -> &Publisher<HeadChange> {
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
        if tsk.cids().is_empty() {
            return Ok(self.heaviest_tipset().await.unwrap());
        }
        tipset_from_keys(&self.ts_cache, self.blockstore(), tsk).await
    }

    /// Determines if provided tipset is heavier than existing known heaviest tipset
    async fn update_heaviest(&self, ts: Arc<Tipset>) -> Result<(), Error> {
        // Calculate heaviest weight before matching to avoid deadlock with mutex
        let heaviest_weight = self
            .heaviest
            .read()
            .await
            .as_ref()
            .map(|ts| weight(self.db.as_ref(), ts.as_ref()));
        match heaviest_weight {
            Some(heaviest) => {
                let new_weight = weight(self.blockstore(), ts.as_ref())?;
                let curr_weight = heaviest?;
                if new_weight > curr_weight {
                    // TODO potentially need to deal with re-orgs here
                    info!("New heaviest tipset: {:?}", ts.key());
                    self.set_heaviest_tipset(ts).await?;
                }
            }
            None => {
                info!("set heaviest tipset");
                self.set_heaviest_tipset(ts).await?;
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

    /// Returns the tipset behind `tsk` at a given `height`.
    /// If the given height is a null round:
    /// - If `prev` is `true`, the tipset before the null round is returned.
    /// - If `prev` is `false`, the tipset following the null round is returned.
    ///
    /// Returns `None` if the tipset provided was the tipset at the given height.
    pub async fn tipset_by_height(
        &self,
        height: ChainEpoch,
        ts: Arc<Tipset>,
        prev: bool,
    ) -> Result<Arc<Tipset>, Error> {
        if height > ts.epoch() {
            return Err(Error::Other(
                "searching for tipset that has a height less than starting point".to_owned(),
            ));
        }
        if height == ts.epoch() {
            return Ok(ts.clone());
        }

        let mut lbts = self
            .chain_index
            .get_tipset_by_height(ts.clone(), height)
            .await?;

        if lbts.epoch() < height {
            warn!(
                "chain index returned the wrong tipset at height {}, using slow retrieval",
                height
            );
            lbts = self
                .chain_index
                .get_tipset_by_height_without_cache(ts, height)
                .await?;
        }

        if lbts.epoch() == height || !prev {
            Ok(lbts)
        } else {
            self.tipset_from_keys(lbts.parents()).await
        }
    }

    pub async fn get_chain_randomness_looking_forward(
        &self,
        blocks: &TipsetKeys,
        pers: DomainSeparationTag,
        round: ChainEpoch,
        entropy: &[u8],
    ) -> Result<[u8; 32], Box<dyn std::error::Error>> {
        self.get_chain_randomness(blocks, pers, round, entropy, false)
            .await
    }

    pub async fn get_chain_randomness_looking_backward(
        &self,
        blocks: &TipsetKeys,
        pers: DomainSeparationTag,
        round: ChainEpoch,
        entropy: &[u8],
    ) -> Result<[u8; 32], Box<dyn std::error::Error>> {
        self.get_chain_randomness(blocks, pers, round, entropy, true)
            .await
    }

    /// Gets 32 bytes of randomness for ChainRand paramaterized by the DomainSeparationTag, ChainEpoch,
    /// Entropy from the ticket chain.
    pub async fn get_chain_randomness(
        &self,
        blocks: &TipsetKeys,
        pers: DomainSeparationTag,
        round: ChainEpoch,
        entropy: &[u8],
        lookback: bool,
    ) -> Result<[u8; 32], Box<dyn std::error::Error>> {
        let ts = self.tipset_from_keys(blocks).await?;

        if round > ts.epoch() {
            return Err("cannot draw randomness from the future".into());
        }

        let search_height = if round < 0 { 0 } else { round };

        let rand_ts = self.tipset_by_height(search_height, ts, lookback).await?;

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

    pub async fn get_beacon_randomness_looking_forward(
        &self,
        blocks: &TipsetKeys,
        pers: DomainSeparationTag,
        round: ChainEpoch,
        entropy: &[u8],
    ) -> Result<[u8; 32], Box<dyn std::error::Error>> {
        self.get_beacon_randomness(blocks, pers, round, entropy, false)
            .await
    }

    pub async fn get_beacon_randomness_looking_backward(
        &self,
        blocks: &TipsetKeys,
        pers: DomainSeparationTag,
        round: ChainEpoch,
        entropy: &[u8],
    ) -> Result<[u8; 32], Box<dyn std::error::Error>> {
        self.get_beacon_randomness(blocks, pers, round, entropy, true)
            .await
    }

    /// Gets 32 bytes of randomness for ChainRand paramaterized by the DomainSeparationTag, ChainEpoch,
    /// Entropy from the latest beacon entry.
    pub async fn get_beacon_randomness(
        &self,
        blocks: &TipsetKeys,
        pers: DomainSeparationTag,
        round: ChainEpoch,
        entropy: &[u8],
        lookback: bool,
    ) -> Result<[u8; 32], Box<dyn std::error::Error>> {
        let ts = self.tipset_from_keys(blocks).await?;

        if round > ts.epoch() {
            return Err("cannot draw randomness from the future".into());
        }

        let search_height = if round < 0 { 0 } else { round };

        let rand_ts = self.tipset_by_height(search_height, ts, lookback).await?;

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
    pub fn fill_tipset(&self, ts: &Tipset) -> Option<FullTipset>
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
                trace!("failed to fill tipset: {}", e);
                return None;
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
        Some(FullTipset::new(blocks).unwrap())
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

    async fn parent_state_tsk<'a>(&self, key: &TipsetKeys) -> Result<StateTree<'_, DB>, Error> {
        let ts = self.tipset_from_keys(key).await?;
        StateTree::new_from_root(&*self.db, ts.parent_state())
            .map_err(|e| Error::Other(format!("Could not get actor state {:?}", e)))
    }

    /// Retrieves ordered valid messages from a `Tipset`. This will only include messages that will
    /// be passed through the VM.
    pub fn messages_for_tipset(&self, ts: &Tipset) -> Result<Vec<ChainMessage>, Error> {
        let bmsgs = self.block_msgs_for_tipset(ts)?;
        Ok(bmsgs.into_iter().map(|bm| bm.messages).flatten().collect())
    }

    /// get miner state given address and tipsetkeys
    pub async fn miner_load_actor_tsk(
        &self,
        address: &Address,
        tsk: &TipsetKeys,
    ) -> Result<miner::State, Error> {
        let state = self.parent_state_tsk(tsk).await?;
        let actor = state
            .get_actor(address)
            .map_err(|_| Error::Other("Failure getting actor".to_string()))?
            .ok_or_else(|| Error::Other("Could not init State Tree".to_string()))?;

        Ok(miner::State::load(self.blockstore(), &actor)?)
    }

    /// Exports a range of tipsets, as well as the state roots based on the `recent_roots`.
    pub async fn export<W>(
        &self,
        tipset: &Tipset,
        recent_roots: ChainEpoch,
        skip_old_msgs: bool,
        mut writer: W,
    ) -> Result<(), Error>
    where
        W: AsyncWrite + Send + Unpin + 'static,
    {
        // Channel cap is equal to buffered write size
        const CHANNEL_CAP: usize = 1000;
        let (tx, mut rx) = bounded(CHANNEL_CAP);
        let header = CarHeader::from(tipset.key().cids().to_vec());

        // Spawns task which receives blocks to write to the car writer.
        let write_task =
            task::spawn(async move { header.write_stream_async(&mut writer, &mut rx).await });

        let global_pre_time = SystemTime::now();
        info!("chain export started");

        // Walks over tipset and historical data, sending all blocks visited into the car writer.
        Self::walk_snapshot(tipset, recent_roots, skip_old_msgs, |cid| {
            let block = self
                .blockstore()
                .get_bytes(&cid)?
                .ok_or_else(|| format!("Cid {} not found in blockstore", cid))?;

            // * If cb can return a generic type, deserializing would remove need to clone.
            // Ignore error intentionally, if receiver dropped, error will be handled below
            let _ = task::block_on(tx.send((cid, block.clone())));
            Ok(block)
        })
        .await?;

        // Drop sender, to close the channel to write task, which will end when finished writing
        drop(tx);

        // Await on values being written.
        write_task
            .await
            .map_err(|e| Error::Other(format!("Failed to write blocks in export: {}", e)))?;

        let time = SystemTime::now()
            .duration_since(global_pre_time)
            .expect("time cannot go backwards");
        info!("export finished, took {} seconds", time.as_secs());

        Ok(())
    }

    /// Subscribes to head changes. This function will send the current tipset through a channel,
    /// start a task that listens to each head change event and forwards into the channel,
    /// then returns an id corresponding to the receiver of this channel from the function.
    /// This function is not blocking on events, and does not stall publishing events as it will
    /// skip over lagged events.
    pub async fn sub_head_changes(&self) -> i64 {
        let (tx, rx) = channel::bounded(16);
        let mut subscriber = self.publisher.subscribe();

        debug!("Reading subscription length");
        let sub_id = self.subscriptions_count.load() as i64 + 1;

        debug!("Incrementing subscriptions count");
        self.subscriptions_count.fetch_add(1);

        debug!("Writing subscription receiver");
        self.subscriptions.insert(sub_id, Some(rx));

        debug!("Subscription ID {} created", sub_id);

        // Send current heaviest tipset into receiver as first event.
        if let Some(ts) = self.heaviest_tipset().await {
            debug!("Tipset published");
            tx.send(HeadChange::Current(ts))
                .await
                .expect("Receiver guaranteed to not drop by now")
        }

        let subscriptions = self.subscriptions.clone();
        debug!("Spawning subscription task");

        task::spawn(async move {
            loop {
                match subscriber.recv().await {
                    Ok(change) => {
                        debug!("Received head changes for subscription ID: {}", sub_id);
                        if tx.send(change).await.is_err() {
                            // Subscriber dropped, no need to keep task alive
                            subscriptions.insert(sub_id, None);
                            break;
                        }
                    }
                    Err(RecvError::Lagged(_)) => {
                        // Can keep polling, as long as receiver is not dropped
                        warn!("Subscriber lagged, ignored head change events");
                    }
                    // This can only happen if chain store is dropped, but fine to exit silently
                    // if this ever does happen.
                    Err(RecvError::Closed) => break,
                }
            }
        });

        debug!("Returning subscription ID {}", sub_id);
        sub_id
    }

    pub async fn next_head_change(&self, sub_id: &i64) -> Option<HeadChange> {
        debug!(
            "Getting subscription receiver for subscription ID: {}",
            sub_id
        );
        if let Some(sub) = self.subscriptions.get(sub_id) {
            if let Some(rx) = sub.val() {
                debug!("Polling receiver for subscription ID: {}", sub_id);
                match rx.recv().await {
                    Ok(head_change) => {
                        debug!("Got head change for subscription ID: {}", sub_id);
                        Some(head_change)
                    }
                    Err(_) => None,
                }
            } else {
                warn!("A subscription with this ID no longer exists");
                None
            }
        } else {
            warn!("No subscription with this ID");
            None
        }
    }

    /// Walks over tipset and state data and loads all blocks not yet seen.
    /// This is tracked based on the callback function loading blocks.
    async fn walk_snapshot<F>(
        tipset: &Tipset,
        recent_roots: ChainEpoch,
        skip_old_msgs: bool,
        mut load_block: F,
    ) -> Result<(), Error>
    where
        F: FnMut(Cid) -> Result<Vec<u8>, Box<dyn StdError>>,
    {
        let mut seen = HashSet::<Cid>::new();
        let mut blocks_to_walk: VecDeque<Cid> = tipset.cids().to_vec().into();
        let mut current_min_height = tipset.epoch();
        let incl_roots_epoch = tipset.epoch() - recent_roots;

        while let Some(next) = blocks_to_walk.pop_front() {
            if !seen.insert(next) {
                continue;
            }

            let data = load_block(next)?;

            let h = BlockHeader::unmarshal_cbor(&data)?;

            if current_min_height > h.epoch() {
                current_min_height = h.epoch();
            }

            if !skip_old_msgs || h.epoch() > incl_roots_epoch {
                recurse_links(&mut seen, *h.messages(), &mut load_block)?;
            }

            if h.epoch() > 0 {
                for p in h.parents().cids() {
                    blocks_to_walk.push_back(*p);
                }
            } else {
                for p in h.parents().cids() {
                    load_block(*p)?;
                }
            }

            if h.epoch() == 0 || h.epoch() > incl_roots_epoch {
                recurse_links(&mut seen, *h.state_root(), &mut load_block)?;
            }
        }
        Ok(())
    }
}

pub(crate) type TipsetCache = RwLock<LruCache<TipsetKeys, Arc<Tipset>>>;

/// Loads a tipset from memory given the tipset keys and cache.
pub(crate) async fn tipset_from_keys<BS>(
    cache: &TipsetCache,
    store: &BS,
    tsk: &TipsetKeys,
) -> Result<Arc<Tipset>, Error>
where
    BS: BlockStore + Send + Sync + 'static,
{
    if let Some(ts) = cache.write().await.get(tsk) {
        return Ok(ts.clone());
    }

    let block_headers: Vec<BlockHeader> = tsk
        .cids()
        .iter()
        .map(|c| {
            store
                .get(c)
                .map_err(|e| Error::Other(e.to_string()))?
                .ok_or_else(|| Error::NotFound(String::from("Key for header")))
        })
        .collect::<Result<_, Error>>()?;

    // construct new Tipset to return
    let ts = Arc::new(Tipset::new(block_headers)?);
    cache.write().await.put(tsk.clone(), ts.clone());
    Ok(ts)
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
        Err(Error::UndefinedKey(format!(
            "no msg root with cid {}",
            msg_cid
        )))
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
    db.put(&header, Blake2b256)
        .map_err(|e| Error::Other(e.to_string()))
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

/// Computes a pseudorandom 32 byte Vec.
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

/// Returns the genesis block from storage.
pub fn genesis<DB>(db: &DB) -> Result<Option<BlockHeader>, Error>
where
    DB: BlockStore,
{
    Ok(db
        .read(GENESIS_KEY)?
        .map(|bz| BlockHeader::unmarshal_cbor(&bz))
        .transpose()?)
}

/// Attempts to deserialize to unsigend message or signed message and then returns it at as a
// [ChainMessage].
pub fn get_chain_message<DB>(db: &DB, key: &Cid) -> Result<ChainMessage, Error>
where
    DB: BlockStore,
{
    db.get(key)
        .map_err(|e| Error::Other(e.to_string()))?
        .ok_or_else(|| Error::UndefinedKey(key.to_string()))
}

/// Given a tipset this function will return all unique messages in that tipset.
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

/// Returns messages from key-value store based on a slice of [Cid]s.
pub fn messages_from_cids<DB, T>(db: &DB, keys: &[Cid]) -> Result<Vec<T>, Error>
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

/// Returns parent message receipt given block_header and message index.
pub fn get_parent_reciept<DB>(
    db: &DB,
    block_header: &BlockHeader,
    i: usize,
) -> Result<Option<MessageReceipt>, Error>
where
    DB: BlockStore,
{
    let amt = Amt::load(block_header.message_receipts(), db)?;
    let receipts = amt.get(i as u64)?;
    Ok(receipts.cloned())
}

/// Returns the weight of provided [Tipset]. This function will load power actor state
/// and calculate the total weight of the [Tipset].
pub fn weight<DB>(db: &DB, ts: &Tipset) -> Result<BigInt, String>
where
    DB: BlockStore,
{
    let state = StateTree::new_from_root(db, ts.parent_state()).map_err(|e| e.to_string())?;

    let act = state
        .get_actor(power::ADDRESS)
        .map_err(|e| e.to_string())?
        .ok_or("Failed to load power actor for calculating weight")?;

    let state = power::State::load(db, &act).map_err(|e| e.to_string())?;

    let tpow = state.into_total_quality_adj_power();

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

#[cfg(feature = "json")]
pub mod headchange_json {
    use super::*;
    use blocks::tipset_json::TipsetJson;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Deserialize, Serialize)]
    #[serde(rename_all = "lowercase")]
    #[serde(tag = "type", content = "val")]
    pub enum HeadChangeJson {
        Current(TipsetJson),
        Apply(TipsetJson),
        Revert(TipsetJson),
    }

    pub type SubscriptionHeadChange = (i64, Vec<HeadChangeJson>);

    impl From<HeadChange> for HeadChangeJson {
        fn from(wrapper: HeadChange) -> Self {
            match wrapper {
                HeadChange::Current(tipset) => HeadChangeJson::Current(TipsetJson(tipset)),
                HeadChange::Apply(tipset) => HeadChangeJson::Apply(TipsetJson(tipset)),
                HeadChange::Revert(tipset) => HeadChangeJson::Revert(TipsetJson(tipset)),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use address::Address;
    use async_std::sync::Arc;
    use cid::Code::{Blake2b256, Identity};

    #[test]
    fn genesis_test() {
        let db = db::MemoryDB::default();

        let cs = ChainStore::new(Arc::new(db));
        let gen_block = BlockHeader::builder()
            .epoch(1)
            .weight(2_u32.into())
            .messages(cid::new_from_cbor(&[], Identity))
            .message_receipts(cid::new_from_cbor(&[], Identity))
            .state_root(cid::new_from_cbor(&[], Identity))
            .miner_address(Address::new_id(0))
            .build()
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
