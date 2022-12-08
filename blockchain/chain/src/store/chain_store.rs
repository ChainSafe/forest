// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::index::checkpoint_tipsets;
use super::{index::ChainIndex, tipset_tracker::TipsetTracker, Error};
use crate::Scale;
use async_stream::stream;
use bls_signatures::Serialize as SerializeBls;
use cid::{multihash::Code::Blake2b256, Cid};
use digest::Digest;
use forest_actor_interface::{miner, EPOCHS_IN_DAY};
use forest_beacon::{BeaconEntry, IGNORE_DRAND_VAR};
use forest_blocks::{Block, BlockHeader, FullTipset, Tipset, TipsetKeys, TxMeta};
use forest_db::Store;
use forest_encoding::de::DeserializeOwned;
use forest_interpreter::BlockMessages;
use forest_ipld::recurse_links;
use forest_legacy_ipld_amt::Amt;
use forest_message::Message as MessageTrait;
use forest_message::{ChainMessage, SignedMessage};
use forest_metrics::metrics;
use forest_utils::db::BlockstoreExt;
use forest_utils::io::Checksum;
use futures::Future;
use fvm::state_tree::StateTree;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_car::CarHeader;
use fvm_ipld_encoding::{from_slice, Cbor};
use fvm_shared::address::Address;
use fvm_shared::clock::ChainEpoch;
use fvm_shared::crypto::signature::{Signature, SignatureType};
use fvm_shared::econ::TokenAmount;
use fvm_shared::message::Message;
use fvm_shared::receipt::Receipt;
use log::{debug, info, trace, warn};
use lru::LruCache;
use serde::Serialize;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::{
    collections::{HashMap, HashSet, VecDeque},
    time::SystemTime,
};
use tokio::io::AsyncWrite;
use tokio::sync::broadcast::{self, Sender as Publisher};
use tokio::sync::{Mutex, RwLock};
use tokio_util::compat::TokioAsyncWriteCompatExt;

const GENESIS_KEY: &str = "gen_block";
const HEAD_KEY: &str = "head";
const BLOCK_VAL_PREFIX: &[u8] = b"block_val/";

// A cap on the size of the future_sink
const SINK_CAP: usize = 200;

const DEFAULT_TIPSET_CACHE_SIZE: NonZeroUsize =
    forest_utils::const_option!(NonZeroUsize::new(8192));

/// `Enum` for `pubsub` channel that defines message type variant and data contained in message type.
#[derive(Clone, Debug)]
pub enum HeadChange {
    Current(Arc<Tipset>),
    Apply(Arc<Tipset>),
    Revert(Arc<Tipset>),
}

/// Stores chain data such as heaviest tipset and cached tipset info at each epoch.
/// This structure is thread-safe, and all caches are wrapped in a mutex to allow a consistent
/// `ChainStore` to be shared across tasks.
pub struct ChainStore<DB> {
    /// Publisher for head change events
    publisher: Publisher<HeadChange>,

    /// key-value `datastore`.
    pub db: DB,

    /// Tipset at the head of the best-known chain.
    heaviest: RwLock<Option<Arc<Tipset>>>,

    /// Caches loaded tipsets for fast retrieval.
    ts_cache: Arc<TipsetCache>,

    /// Used as a cache for tipset `lookbacks`.
    chain_index: ChainIndex<DB>,

    /// Tracks blocks for the purpose of forming tipsets.
    tipset_tracker: TipsetTracker<DB>,
}

impl<DB> ChainStore<DB>
where
    DB: Blockstore + Store + Send + Sync,
{
    pub async fn new(db: DB) -> Self
    where
        DB: Clone,
    {
        let (publisher, _) = broadcast::channel(SINK_CAP);
        let ts_cache = Arc::new(RwLock::new(LruCache::new(DEFAULT_TIPSET_CACHE_SIZE)));
        let cs = Self {
            publisher,
            // subscriptions: Default::default(),
            // subscriptions_count: Default::default(),
            chain_index: ChainIndex::new(ts_cache.clone(), db.clone()),
            tipset_tracker: TipsetTracker::new(db.clone()),
            db,
            ts_cache,
            heaviest: Default::default(),
        };

        // Result intentionally ignored, doesn't matter if heaviest doesn't exist in store yet
        let _ = cs.load_heaviest_tipset().await;

        cs
    }

    /// Sets heaviest tipset within `ChainStore` and store its tipset keys under `HEAD_KEY`
    pub async fn set_heaviest_tipset(&self, ts: Arc<Tipset>) -> Result<(), Error> {
        self.db.write(HEAD_KEY, ts.key().marshal_cbor()?)?;
        *self.heaviest.write().await = Some(ts.clone());
        if self.publisher.send(HeadChange::Apply(ts)).is_err() {
            debug!("did not publish head change, no active receivers");
        }
        Ok(())
    }

    /// Writes genesis to `blockstore`.
    pub fn set_genesis(&self, header: &BlockHeader) -> Result<Cid, Error> {
        set_genesis(self.blockstore(), header)
    }

    /// Adds a [`BlockHeader`] to the tipset tracker, which tracks valid headers.
    pub async fn add_to_tipset_tracker(&self, header: &BlockHeader) {
        self.tipset_tracker.add(header).await;
    }

    /// Writes tipset block headers to data store and updates heaviest tipset with other
    /// compatible tracked headers.
    pub async fn put_tipset<S>(&self, ts: &Tipset) -> Result<(), Error>
    where
        S: Scale,
    {
        // TODO: we could add the blocks of `ts` to the tipset tracker from here,
        // making `add_to_tipset_tracker` redundant and decreasing the number of
        // `blockstore` reads
        persist_objects(self.blockstore(), ts.blocks())?;

        // Expand tipset to include other compatible blocks at the epoch.
        let expanded = self.expand_tipset(ts.min_ticket_block().clone()).await?;
        self.update_heaviest::<S>(Arc::new(expanded)).await?;
        Ok(())
    }

    /// Expands tipset to tipset with all other headers in the same epoch using the tipset tracker.
    async fn expand_tipset(&self, header: BlockHeader) -> Result<Tipset, Error> {
        self.tipset_tracker.expand(header).await
    }

    /// Loads heaviest tipset from `datastore` and sets as heaviest in `chainstore`.
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

    /// Returns genesis [`BlockHeader`] from the store based on a static key.
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

    /// Returns Tipset from key-value store from provided CIDs
    pub async fn tipset_from_keys(&self, tsk: &TipsetKeys) -> Result<Arc<Tipset>, Error> {
        if tsk.cids().is_empty() {
            return Ok(self.heaviest_tipset().await.unwrap());
        }
        tipset_from_keys(&self.ts_cache, self.blockstore(), tsk).await
    }

    /// Returns Tipset key hash from key-value store from provided CIDs
    pub async fn tipset_hash_from_keys(&self, tsk: &TipsetKeys) -> String {
        checkpoint_tipsets::tipset_hash(tsk)
    }

    /// Determines if provided tipset is heavier than existing known heaviest tipset
    async fn update_heaviest<S>(&self, ts: Arc<Tipset>) -> Result<(), Error>
    where
        S: Scale,
    {
        // Calculate heaviest weight before matching to avoid deadlock with mutex
        let heaviest_weight = self
            .heaviest
            .read()
            .await
            .as_ref()
            .map(|ts| S::weight(self.blockstore(), ts.as_ref()));

        match heaviest_weight {
            Some(heaviest) => {
                let new_weight = S::weight(self.blockstore(), ts.as_ref())?;
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

        Ok(self.db.write(key, [])?)
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

    pub async fn validate_tipset_checkpoints(
        &self,
        from: Arc<Tipset>,
        network: String,
    ) -> Result<(), Error> {
        info!(
            "Validating {network} tipset checkpoint hashes from: {}",
            from.epoch()
        );

        let Some(mut hashes) = checkpoint_tipsets::get_tipset_hashes(&network) else {
            info!("No checkpoint tipsets found for network: {network}, skipping validation.");
            return Ok(());
        };

        let mut ts = from;
        let tipset_hash = checkpoint_tipsets::tipset_hash(ts.key());
        hashes.remove(&tipset_hash);

        loop {
            let pts = self.chain_index.load_tipset(ts.parents()).await?;
            let tipset_hash = checkpoint_tipsets::tipset_hash(ts.key());
            hashes.remove(&tipset_hash);

            ts = pts;

            if ts.epoch() == 0 {
                break;
            }
        }

        if !hashes.is_empty() {
            return Err(Error::Other(format!(
                "Found tipset hash(es) on {network} that are no longer valid: {:?}",
                hashes
            )));
        }

        if !checkpoint_tipsets::validate_genesis_cid(ts.key(), &network) {
            return Err(Error::Other(format!(
                "Genesis cid {:?} on {network} network does not match with one stored in checkpoint registry",
                ts.key().cid()
            )));
        }

        Ok(())
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
        DB: Blockstore,
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
    ///
    /// It removes duplicate messages which appear in multiple blocks.
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

    async fn parent_state_tsk(&self, key: &TipsetKeys) -> anyhow::Result<StateTree<&DB>, Error> {
        let ts = self.tipset_from_keys(key).await?;
        StateTree::new_from_root(self.blockstore(), ts.parent_state())
            .map_err(|e| Error::Other(format!("Could not get actor state {:?}", e)))
    }

    /// Retrieves ordered valid messages from a `Tipset`. This will only include messages that will
    /// be passed through the VM.
    pub fn messages_for_tipset(&self, ts: &Tipset) -> Result<Vec<ChainMessage>, Error> {
        let bmsgs = self.block_msgs_for_tipset(ts)?;
        Ok(bmsgs.into_iter().flat_map(|bm| bm.messages).collect())
    }

    /// get miner state given address and tipsetkeys
    pub async fn miner_load_actor_tsk(
        &self,
        address: &Address,
        tsk: &TipsetKeys,
    ) -> anyhow::Result<miner::State> {
        let state = self.parent_state_tsk(tsk).await?;
        let actor = state
            .get_actor(address)
            .map_err(|_| Error::Other("Failure getting actor".to_string()))?
            .ok_or_else(|| Error::Other("Could not init State Tree".to_string()))?;

        miner::State::load(self.blockstore(), &actor)
    }

    /// Exports a range of tipsets, as well as the state roots based on the `recent_roots`.
    pub async fn export<W, D>(
        &self,
        tipset: &Tipset,
        recent_roots: ChainEpoch,
        writer: W,
    ) -> Result<digest::Output<D>, Error>
    where
        D: Digest,
        W: AsyncWrite + Checksum<D> + Send + Unpin + 'static,
    {
        // Channel cap is equal to buffered write size
        const CHANNEL_CAP: usize = 1000;
        let (tx, rx) = flume::bounded(CHANNEL_CAP);
        let header = CarHeader::from(tipset.key().cids().to_vec());

        let writer = Arc::new(Mutex::new(writer.compat_write()));
        let writer_clone = writer.clone();

        // Spawns task which receives blocks to write to the car writer.
        let write_task = tokio::task::spawn(async move {
            let mut writer = writer_clone.lock().await;
            header
                .write_stream_async(
                    &mut *writer,
                    &mut Box::pin(stream! {
                        while let Ok(val) = rx.recv_async().await {
                            yield val;
                        }
                    }),
                )
                .await
                .map_err(|e| Error::Other(format!("Failed to write blocks in export: {e}")))
        });

        let global_pre_time = SystemTime::now();
        info!("chain export started");

        // Walks over tipset and historical data, sending all blocks visited into the car writer.
        Self::walk_snapshot(tipset, recent_roots, |cid| {
            let tx_clone = tx.clone();
            async move {
                let block = self
                    .blockstore()
                    .get(&cid)?
                    .ok_or_else(|| Error::Other("Cid {cid} not found in blockstore".to_string()))?;

                tx_clone.send_async((cid, block.clone())).await?;
                Ok(block)
            }
        })
        .await?;

        // Drop sender, to close the channel to write task, which will end when finished writing
        drop(tx);

        // Await on values being written.
        write_task
            .await
            .map_err(|e| Error::Other(format!("Failed to write blocks in export: {e}")))??;

        let time = SystemTime::now()
            .duration_since(global_pre_time)
            .expect("time cannot go backwards");
        info!("export finished, took {} seconds", time.as_secs());

        let digest = writer.lock().await.get_mut().finalize();
        Ok(digest)
    }

    /// Walks over tipset and state data and loads all blocks not yet seen.
    /// This is tracked based on the callback function loading blocks.
    async fn walk_snapshot<F, T>(
        tipset: &Tipset,
        recent_roots: ChainEpoch,
        mut load_block: F,
    ) -> Result<(), Error>
    where
        F: FnMut(Cid) -> T + Send,
        T: Future<Output = Result<Vec<u8>, anyhow::Error>> + Send,
    {
        let mut seen = HashSet::<Cid>::new();
        let mut blocks_to_walk: VecDeque<Cid> = tipset.cids().to_vec().into();
        let mut current_min_height = tipset.epoch();
        let incl_roots_epoch = tipset.epoch() - recent_roots;

        while let Some(next) = blocks_to_walk.pop_front() {
            if !seen.insert(next) {
                continue;
            }

            let data = load_block(next).await?;

            let h = BlockHeader::unmarshal_cbor(&data)?;

            if current_min_height > h.epoch() {
                current_min_height = h.epoch();
                if current_min_height % EPOCHS_IN_DAY == 0 {
                    info!(target: "chain_api", "export at: {}", current_min_height);
                }
            }

            if h.epoch() > 0 {
                for p in h.parents().cids() {
                    blocks_to_walk.push_back(*p);
                }
            } else {
                for p in h.parents().cids() {
                    load_block(*p).await?;
                }
            }

            if h.epoch() == 0 || h.epoch() > incl_roots_epoch {
                recurse_links(&mut seen, *h.state_root(), &mut load_block).await?;
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
    BS: Blockstore,
{
    if let Some(ts) = cache.write().await.get(tsk) {
        metrics::LRU_CACHE_HIT
            .with_label_values(&[metrics::values::TIPSET])
            .inc();
        return Ok(ts.clone());
    }

    let block_headers: Vec<BlockHeader> = tsk
        .cids()
        .iter()
        .map(|c| {
            store
                .get_obj(c)
                .map_err(|e| Error::Other(e.to_string()))?
                .ok_or_else(|| Error::NotFound(String::from("Key for header")))
        })
        .collect::<Result<_, Error>>()?;

    // construct new Tipset to return
    let ts = Arc::new(Tipset::new(block_headers)?);
    cache.write().await.put(tsk.clone(), ts.clone());
    metrics::LRU_CACHE_MISS
        .with_label_values(&[metrics::values::TIPSET])
        .inc();
    Ok(ts)
}

/// Helper to ensure consistent CID to db key translation.
fn block_validation_key(cid: &Cid) -> Vec<u8> {
    let mut key = Vec::new();
    key.extend_from_slice(BLOCK_VAL_PREFIX);
    key.extend(cid.to_bytes());
    key
}

/// Returns a Tuple of BLS messages of type `UnsignedMessage` and SECP messages
/// of type `SignedMessage`
pub fn block_messages<DB>(
    db: &DB,
    bh: &BlockHeader,
) -> Result<(Vec<Message>, Vec<SignedMessage>), Error>
where
    DB: Blockstore,
{
    let (bls_cids, secpk_cids) = read_msg_cids(db, bh.messages())?;

    let bls_msgs: Vec<Message> = messages_from_cids(db, &bls_cids)?;
    let secp_msgs: Vec<SignedMessage> = messages_from_cids(db, &secpk_cids)?;

    Ok((bls_msgs, secp_msgs))
}

/// Returns a tuple of `UnsignedMessage` and `SignedMessages` from their CID
pub fn block_messages_from_cids<DB>(
    db: &DB,
    bls_cids: &[Cid],
    secp_cids: &[Cid],
) -> Result<(Vec<Message>, Vec<SignedMessage>), Error>
where
    DB: Blockstore,
{
    let bls_msgs: Vec<Message> = messages_from_cids(db, bls_cids)?;
    let secp_msgs: Vec<SignedMessage> = messages_from_cids(db, secp_cids)?;

    Ok((bls_msgs, secp_msgs))
}

/// Returns a tuple of CIDs for both unsigned and signed messages
// TODO cache these recent meta roots
pub fn read_msg_cids<DB>(db: &DB, msg_cid: &Cid) -> Result<(Vec<Cid>, Vec<Cid>), Error>
where
    DB: Blockstore,
{
    if let Some(roots) = db
        .get_obj::<TxMeta>(msg_cid)
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

/// Sets the genesis key in the `Blockstore`. Be careful if using this outside of
/// the `ChainStore` as it will not update what the `ChainStore` thinks is the genesis
/// after the `ChainStore` has been created.
pub fn set_genesis<DB>(db: &DB, header: &BlockHeader) -> Result<Cid, Error>
where
    DB: Blockstore + Store,
{
    db.write(GENESIS_KEY, header.marshal_cbor()?)?;
    db.put_obj(&header, Blake2b256)
        .map_err(|e| Error::Other(e.to_string()))
}

/// Persists slice of `serializable` objects to `blockstore`.
pub fn persist_objects<DB, C>(db: &DB, headers: &[C]) -> Result<(), Error>
where
    DB: Blockstore,
    C: Serialize,
{
    for chunk in headers.chunks(256) {
        db.bulk_put(chunk, Blake2b256)
            .map_err(|e| Error::Other(e.to_string()))?;
    }
    Ok(())
}

/// Returns a vector of CIDs from provided root CID
fn read_amt_cids<DB>(db: &DB, root: &Cid) -> Result<Vec<Cid>, Error>
where
    DB: Blockstore,
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
    DB: Blockstore + Store,
{
    Ok(db
        .read(GENESIS_KEY)?
        .map(|bz| BlockHeader::unmarshal_cbor(&bz))
        .transpose()?)
}

/// Attempts to de-serialize to unsigned message or signed message and then returns it as a
/// [`ChainMessage`].
pub fn get_chain_message<DB>(db: &DB, key: &Cid) -> Result<ChainMessage, Error>
where
    DB: Blockstore,
{
    db.get_obj(key)
        .map_err(|e| Error::Other(e.to_string()))?
        .ok_or_else(|| Error::UndefinedKey(key.to_string()))
}

/// Given a tipset this function will return all unique messages in that tipset.
pub fn messages_for_tipset<DB>(db: &DB, ts: &Tipset) -> Result<Vec<ChainMessage>, Error>
where
    DB: Blockstore,
{
    let mut applied: HashMap<Address, u64> = HashMap::new();
    let mut balances: HashMap<Address, TokenAmount> = HashMap::new();
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
            if applied.contains_key(from_address) {
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

/// Returns messages from key-value store based on a slice of [`Cid`]s.
pub fn messages_from_cids<DB, T>(db: &DB, keys: &[Cid]) -> Result<Vec<T>, Error>
where
    DB: Blockstore,
    T: DeserializeOwned,
{
    keys.iter()
        .map(|k| {
            db.get_obj(k)
                .map_err(|e| Error::Other(e.to_string()))?
                .ok_or_else(|| Error::UndefinedKey(k.to_string()))
        })
        .collect()
}

/// Returns parent message receipt given `block_header` and message index.
pub fn get_parent_reciept<DB>(
    db: &DB,
    block_header: &BlockHeader,
    i: usize,
) -> Result<Option<Receipt>, Error>
where
    DB: Blockstore,
{
    let amt = Amt::load(block_header.message_receipts(), db)?;
    let receipts = amt.get(i)?;
    Ok(receipts.cloned())
}

pub mod headchange_json {
    use super::*;
    use forest_blocks::tipset_json::TipsetJson;
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

/// Result of persisting a vector of `SignedMessage`s that are to be included in a block.
///
/// The fields are public so they can be partially moved, but they should not be modified.
pub struct PersistedBlockMessages {
    /// Overall CID to be included in the `BlockHeader`.
    pub msg_cid: Cid,
    /// All CIDs of SECP messages, to be included in `BlockMsg`.
    pub secp_cids: Vec<Cid>,
    /// All CIDs of BLS messages, to be included in `BlockMsg`.
    pub bls_cids: Vec<Cid>,
    /// Aggregated signature of all BLS messages, to be included in the `BlockHeader`.
    pub bls_agg: Signature,
}

/// Partition the messages into SECP and BLS variants, store them individually in the IPLD store,
/// and the corresponding `TxMeta` as well, returning its CID so that it can be put in a block header.
/// Also return the aggregated BLS signature of all BLS messages.
pub fn persist_block_messages<DB: Blockstore>(
    db: &DB,
    messages: Vec<&SignedMessage>,
) -> anyhow::Result<PersistedBlockMessages> {
    let mut bls_cids = Vec::new();
    let mut secp_cids = Vec::new();

    let mut bls_sigs = Vec::new();
    for msg in messages {
        if msg.signature().signature_type() == SignatureType::BLS {
            let c = db.put_obj(&msg.message, Blake2b256)?;
            bls_cids.push(c);
            bls_sigs.push(&msg.signature);
        } else {
            let c = db.put_obj(&msg, Blake2b256)?;
            secp_cids.push(c);
        }
    }

    let bls_msg_root = Amt::new_from_iter(db, bls_cids.iter().copied())?;
    let secp_msg_root = Amt::new_from_iter(db, secp_cids.iter().copied())?;

    let mmcid = db.put_obj(
        &TxMeta {
            bls_message_root: bls_msg_root,
            secp_message_root: secp_msg_root,
        },
        Blake2b256,
    )?;

    let bls_agg = if bls_sigs.is_empty() {
        Signature::new_bls(vec![])
    } else {
        Signature::new_bls(
            bls_signatures::aggregate(
                &bls_sigs
                    .iter()
                    .map(|s| s.bytes())
                    .map(bls_signatures::Signature::from_bytes)
                    .collect::<Result<Vec<_>, _>>()?,
            )
            .unwrap()
            .as_bytes(),
        )
    };

    Ok(PersistedBlockMessages {
        msg_cid: mmcid,
        secp_cids,
        bls_cids,
        bls_agg,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use cid::multihash::Code::{Blake2b256, Identity};
    use cid::multihash::MultihashDigest;
    use cid::Cid;
    use fvm_ipld_encoding::DAG_CBOR;
    use fvm_shared::address::Address;

    #[tokio::test]
    async fn genesis_test() {
        let db = forest_db::MemoryDB::default();

        let cs = ChainStore::new(db).await;
        let gen_block = BlockHeader::builder()
            .epoch(1)
            .weight(2_u32.into())
            .messages(Cid::new_v1(DAG_CBOR, Identity.digest(&[])))
            .message_receipts(Cid::new_v1(DAG_CBOR, Identity.digest(&[])))
            .state_root(Cid::new_v1(DAG_CBOR, Identity.digest(&[])))
            .miner_address(Address::new_id(0))
            .build()
            .unwrap();

        assert_eq!(cs.genesis().unwrap(), None);
        cs.set_genesis(&gen_block).unwrap();
        assert_eq!(cs.genesis().unwrap(), Some(gen_block));
    }

    #[tokio::test]
    async fn block_validation_cache_basic() {
        let db = forest_db::MemoryDB::default();

        let cs = ChainStore::new(db).await;

        let cid = Cid::new_v1(DAG_CBOR, Blake2b256.digest(&[1, 2, 3]));
        assert!(!cs.is_block_validated(&cid).unwrap());

        cs.mark_block_as_validated(&cid).unwrap();
        assert!(cs.is_block_validated(&cid).unwrap());
    }
}
