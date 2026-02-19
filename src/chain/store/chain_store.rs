// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{
    Error,
    index::{ChainIndex, ResolveNullTipset},
    tipset_tracker::TipsetTracker,
};
use crate::db::{EthMappingsStore, EthMappingsStoreExt};
use crate::interpreter::{BlockMessages, VMTrace};
use crate::libp2p_bitswap::{BitswapStoreRead, BitswapStoreReadWrite};
use crate::message::{ChainMessage, Message as MessageTrait, SignedMessage};
use crate::networks::{ChainConfig, Height};
use crate::rpc::eth::{eth_tx_from_signed_eth_message, types::EthHash};
use crate::shim::clock::ChainEpoch;
use crate::shim::{
    address::Address, econ::TokenAmount, executor::Receipt, message::Message,
    state_tree::StateTree, version::NetworkVersion,
};
use crate::state_manager::StateOutput;
use crate::utils::db::{BlockstoreExt, CborStoreExt};
use crate::{
    blocks::{CachingBlockHeader, Tipset, TipsetKey, TxMeta},
    db::HeaviestTipsetKeyProvider,
};
use crate::{fil_cns, utils::cache::SizeTrackingLruCache};
use ahash::{HashMap, HashMapExt, HashSet};
use anyhow::Context as _;
use cid::Cid;
use fil_actors_shared::fvm_ipld_amt::Amtv0 as Amt;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::CborStore;
use itertools::Itertools;
use nonzero_ext::nonzero;
use parking_lot::{Mutex, RwLock};
use serde::{Serialize, de::DeserializeOwned};
use std::{num::NonZeroUsize, sync::Arc};
use tokio::sync::broadcast::{self, Sender as Publisher};
use tracing::{debug, trace, warn};

// A cap on the size of the future_sink
const SINK_CAP: usize = 200;

/// Disambiguate the type to signify that we are expecting a delta and not an actual epoch/height
/// while maintaining the same type.
pub type ChainEpochDelta = ChainEpoch;

/// `Enum` for `pubsub` channel that defines message type variant and data
/// contained in message type.
#[derive(Clone, Debug)]
pub enum HeadChange {
    Apply(Tipset),
}

/// Stores chain data such as heaviest tipset and cached tipset info at each
/// epoch. This structure is thread-safe, and all caches are wrapped in a mutex
/// to allow a consistent `ChainStore` to be shared across tasks.
pub struct ChainStore<DB> {
    /// Publisher for head change events
    publisher: Publisher<HeadChange>,

    /// key-value `datastore`.
    db: Arc<DB>,

    /// Heaviest tipset key provider
    heaviest_tipset_key_provider: Arc<dyn HeaviestTipsetKeyProvider + Sync + Send>,

    /// Heaviest tipset cache
    heaviest_tipset_cache: Arc<RwLock<Option<Tipset>>>,

    /// Used as a cache for tipset `lookbacks`.
    chain_index: Arc<ChainIndex<Arc<DB>>>,

    /// Tracks blocks for the purpose of forming tipsets.
    tipset_tracker: TipsetTracker<DB>,

    genesis_block_header: CachingBlockHeader,

    /// validated blocks
    validated_blocks: Mutex<HashSet<Cid>>,

    /// Ethereum mappings store
    eth_mappings: Arc<dyn EthMappingsStore + Sync + Send>,

    /// Needed by the Ethereum mapping.
    chain_config: Arc<ChainConfig>,
}

impl<DB> BitswapStoreRead for ChainStore<DB>
where
    DB: BitswapStoreRead,
{
    fn contains(&self, cid: &Cid) -> anyhow::Result<bool> {
        self.db.contains(cid)
    }

    fn get(&self, cid: &Cid) -> anyhow::Result<Option<Vec<u8>>> {
        self.db.get(cid)
    }
}

impl<DB> BitswapStoreReadWrite for ChainStore<DB>
where
    DB: BitswapStoreReadWrite,
{
    type Hashes = <DB as BitswapStoreReadWrite>::Hashes;

    fn insert(&self, block: &crate::libp2p_bitswap::Block64<Self::Hashes>) -> anyhow::Result<()> {
        self.db.insert(block)
    }
}

impl<DB> ChainStore<DB>
where
    DB: Blockstore,
{
    pub fn new(
        db: Arc<DB>,
        heaviest_tipset_key_provider: Arc<dyn HeaviestTipsetKeyProvider + Sync + Send>,
        eth_mappings: Arc<dyn EthMappingsStore + Sync + Send>,
        chain_config: Arc<ChainConfig>,
        genesis_block_header: CachingBlockHeader,
    ) -> anyhow::Result<Self> {
        let (publisher, _) = broadcast::channel(SINK_CAP);
        let chain_index = Arc::new(ChainIndex::new(Arc::clone(&db)));
        let validated_blocks = Mutex::new(HashSet::default());

        let cs = Self {
            publisher,
            chain_index,
            tipset_tracker: TipsetTracker::new(Arc::clone(&db), chain_config.clone()),
            db,
            heaviest_tipset_key_provider,
            heaviest_tipset_cache: Default::default(),
            genesis_block_header,
            validated_blocks,
            eth_mappings,
            chain_config,
        };

        Ok(cs)
    }

    /// Sets heaviest tipset
    pub fn set_heaviest_tipset(&self, ts: Tipset) -> Result<(), Error> {
        self.heaviest_tipset_key_provider
            .set_heaviest_tipset_key(ts.key())?;
        *self.heaviest_tipset_cache.write() = Some(ts.clone());
        if self.publisher.send(HeadChange::Apply(ts)).is_err() {
            debug!("did not publish head change, no active receivers");
        }
        Ok(())
    }

    /// Adds a block header to the tipset tracker, which tracks valid headers.
    pub fn add_to_tipset_tracker(&self, header: &CachingBlockHeader) {
        self.tipset_tracker.add(header);
    }

    /// Writes tipset block headers to data store and updates heaviest tipset
    /// with other compatible tracked headers.
    pub fn put_tipset(&self, ts: &Tipset) -> Result<(), Error> {
        persist_objects(self.blockstore(), ts.block_headers().iter())?;

        // Expand tipset to include other compatible blocks at the epoch.
        let expanded = self.expand_tipset(ts.min_ticket_block().clone())?;
        self.update_heaviest(expanded)?;
        Ok(())
    }

    /// Writes the `TipsetKey` to the blockstore for `EthAPI` queries.
    pub fn put_tipset_key(&self, tsk: &TipsetKey) -> Result<(), Error> {
        let tsk_bytes = tsk.bytes();
        let tsk_cid = self.blockstore().put_cbor_default(&tsk_bytes)?;
        let hash = tsk_cid.into();
        self.eth_mappings.write_obj(&hash, tsk)?;
        Ok(())
    }

    /// Reads the `TipsetKey` from the blockstore for `EthAPI` queries.
    pub fn get_required_tipset_key(&self, hash: &EthHash) -> Result<TipsetKey, Error> {
        let tsk = self
            .eth_mappings
            .read_obj::<TipsetKey>(hash)?
            .with_context(|| format!("cannot find tipset with hash {hash}"))?;

        Ok(tsk)
    }

    /// Writes with timestamp the `Hash` to `Cid` mapping to the blockstore for `EthAPI` queries.
    pub fn put_mapping(&self, k: EthHash, v: Cid, timestamp: u64) -> Result<(), Error> {
        self.eth_mappings.write_obj(&k, &(v, timestamp))?;
        Ok(())
    }

    /// Reads the `Cid` from the blockstore for `EthAPI` queries.
    pub fn get_mapping(&self, hash: &EthHash) -> Result<Option<Cid>, Error> {
        Ok(self
            .eth_mappings
            .read_obj::<(Cid, u64)>(hash)?
            .map(|(cid, _)| cid))
    }

    /// Expands tipset to tipset with all other headers in the same epoch using
    /// the tipset tracker.
    fn expand_tipset(&self, header: CachingBlockHeader) -> Result<Tipset, Error> {
        self.tipset_tracker.expand(header)
    }

    pub fn genesis_block_header(&self) -> &CachingBlockHeader {
        &self.genesis_block_header
    }

    /// Returns the currently tracked heaviest tipset.
    pub fn heaviest_tipset(&self) -> Tipset {
        if let Some(ts) = &*self.heaviest_tipset_cache.read() {
            return ts.clone();
        }
        let tsk = self
            .heaviest_tipset_key_provider
            .heaviest_tipset_key()
            .unwrap_or_else(|_| TipsetKey::from(nunny::vec![*self.genesis_block_header.cid()]));
        self.chain_index
            .load_required_tipset(&tsk)
            .expect("failed to load heaviest tipset")
    }

    /// Returns the genesis tipset.
    pub fn genesis_tipset(&self) -> Tipset {
        Tipset::from(self.genesis_block_header())
    }

    /// Returns a reference to the publisher of head changes.
    pub fn publisher(&self) -> &Publisher<HeadChange> {
        &self.publisher
    }

    /// Returns key-value store instance.
    pub fn blockstore(&self) -> &Arc<DB> {
        &self.db
    }

    /// Returns the chain index
    pub fn chain_index(&self) -> &Arc<ChainIndex<Arc<DB>>> {
        &self.chain_index
    }

    /// Returns the chain configuration
    pub fn chain_config(&self) -> &Arc<ChainConfig> {
        &self.chain_config
    }

    /// Lotus often treats an empty [`TipsetKey`] as shorthand for "the heaviest tipset".
    /// You may opt-in to that behavior by calling this method with [`None`].
    ///
    /// This calls fails if the tipset is missing or invalid.
    #[tracing::instrument(skip_all)]
    pub fn load_required_tipset_or_heaviest<'a>(
        &self,
        maybe_key: impl Into<Option<&'a TipsetKey>>,
    ) -> Result<Tipset, Error> {
        match maybe_key.into() {
            Some(key) => self.chain_index.load_required_tipset(key),
            None => Ok(self.heaviest_tipset()),
        }
    }

    /// Determines if provided tipset is heavier than existing known heaviest
    /// tipset
    fn update_heaviest(&self, ts: Tipset) -> Result<(), Error> {
        // Calculate heaviest weight before matching to avoid deadlock with mutex
        let heaviest_weight = fil_cns::weight(self.blockstore(), &self.heaviest_tipset())?;

        let new_weight = fil_cns::weight(self.blockstore(), &ts)?;
        let curr_weight = heaviest_weight;

        if new_weight > curr_weight {
            self.set_heaviest_tipset(ts)?;
        }
        Ok(())
    }

    /// Checks metadata file if block has already been validated.
    pub fn is_block_validated(&self, cid: &Cid) -> bool {
        let validated = self.validated_blocks.lock().contains(cid);
        if validated {
            trace!("Block {cid} was previously validated");
        }
        validated
    }

    /// Marks block as validated in the metadata file.
    pub fn mark_block_as_validated(&self, cid: &Cid) {
        let mut file = self.validated_blocks.lock();
        file.insert(*cid);
    }

    pub fn unmark_block_as_validated(&self, cid: &Cid) {
        let mut file = self.validated_blocks.lock();
        let _did_work = file.remove(cid);
    }

    /// Retrieves ordered valid messages from a `Tipset`. This will only include
    /// messages that will be passed through the VM.
    pub fn messages_for_tipset(&self, ts: &Tipset) -> Result<Vec<ChainMessage>, Error> {
        let bmsgs = BlockMessages::for_tipset(&self.db, ts)?;
        Ok(bmsgs.into_iter().flat_map(|bm| bm.messages).collect())
    }

    /// Gets look-back tipset (and state-root of that tipset) for block
    /// validations.
    ///
    /// The look-back tipset for a round is the tipset with epoch `round -
    /// chain_finality`. [Chain
    /// finality](https://docs.filecoin.io/reference/general/glossary/#finality)
    /// is usually 900. The `heaviest_tipset` is a reference point in the
    /// blockchain. It must be a child of the look-back tipset.
    pub fn get_lookback_tipset_for_round(
        chain_index: &Arc<ChainIndex<Arc<DB>>>,
        chain_config: &Arc<ChainConfig>,
        heaviest_tipset: &Tipset,
        round: ChainEpoch,
    ) -> Result<(Tipset, Cid), Error>
    where
        DB: Send + Sync + 'static,
    {
        let version = chain_config.network_version(round);
        let lb = if version <= NetworkVersion::V3 {
            ChainEpoch::from(10)
        } else {
            chain_config.policy.chain_finality
        };
        let lbr = (round - lb).max(0);

        // More null blocks than lookback
        if lbr >= heaviest_tipset.epoch() {
            // This situation is extremely rare so it's fine to compute the
            // state-root without caching.
            let genesis_timestamp = heaviest_tipset.genesis(chain_index.db())?.timestamp;
            let beacon = Arc::new(chain_config.get_beacon_schedule(genesis_timestamp));
            let StateOutput { state_root, .. } = crate::state_manager::apply_block_messages(
                genesis_timestamp,
                Arc::clone(chain_index),
                Arc::clone(chain_config),
                beacon,
                // Using shared WASM engine here as creating new WASM engines is expensive
                // (takes seconds to minutes). It's only acceptable here because this situation is
                // so rare (may happen in dev-networks, doesn't happen in calibnet or mainnet.)
                &crate::shim::machine::GLOBAL_MULTI_ENGINE,
                heaviest_tipset.clone(),
                crate::state_manager::NO_CALLBACK,
                VMTrace::NotTraced,
            )
            .map_err(|e| Error::Other(e.to_string()))?;
            return Ok((heaviest_tipset.clone(), state_root));
        }

        let next_ts = chain_index
            .tipset_by_height(
                lbr + 1,
                heaviest_tipset.clone(),
                ResolveNullTipset::TakeNewer,
            )
            .map_err(|e| Error::Other(format!("Could not get tipset by height {e:?}")))?;
        if lbr > next_ts.epoch() {
            return Err(Error::Other(format!(
                "failed to find non-null tipset {:?} {} which is known to exist, found {:?} {}",
                heaviest_tipset.key(),
                heaviest_tipset.epoch(),
                next_ts.key(),
                next_ts.epoch()
            )));
        }
        let lbts = chain_index
            .load_required_tipset(next_ts.parents())
            .map_err(|e| Error::Other(format!("Could not get tipset from keys {e:?}")))?;
        Ok((lbts, *next_ts.parent_state()))
    }

    /// Filter [`SignedMessage`]'s to keep only the most recent ones, then write corresponding entries to the Ethereum mapping.
    pub fn process_signed_messages(&self, messages: &[(SignedMessage, u64)]) -> anyhow::Result<()>
    where
        DB: fvm_ipld_blockstore::Blockstore,
    {
        let eth_txs: Vec<(EthHash, Cid, u64, usize)> = messages
            .iter()
            .enumerate()
            .filter_map(|(i, (smsg, timestamp))| {
                if let Ok((_, tx)) =
                    eth_tx_from_signed_eth_message(smsg, self.chain_config.eth_chain_id)
                {
                    if let Ok(hash) = tx.eth_hash() {
                        // newest messages are the ones with lowest index
                        Some((hash.into(), smsg.cid(), *timestamp, i))
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect();
        let filtered = filter_lowest_index(eth_txs);
        let num_entries = filtered.len();

        // write back
        for (k, v, timestamp) in filtered.into_iter() {
            tracing::trace!("Insert mapping {} => {}", k, v);
            self.put_mapping(k, v, timestamp)?;
        }
        tracing::trace!("Wrote {} entries in Ethereum mapping", num_entries);
        Ok(())
    }

    pub fn headers_delegated_messages<'a>(
        &self,
        headers: impl Iterator<Item = &'a CachingBlockHeader>,
    ) -> anyhow::Result<Vec<(SignedMessage, u64)>>
    where
        DB: fvm_ipld_blockstore::Blockstore,
    {
        let mut delegated_messages = vec![];

        // Hygge is the start of Ethereum support in the FVM (through the FEVM actor).
        // Before this height, no notion of an Ethereum-like API existed.
        let filtered_headers =
            headers.filter(|bh| bh.epoch >= self.chain_config.epoch(Height::Hygge));

        for bh in filtered_headers {
            if let Ok((_, secp_cids)) = block_messages(self.blockstore(), bh) {
                let mut messages: Vec<_> = secp_cids
                    .into_iter()
                    .filter(|msg| msg.is_delegated())
                    .map(|m| (m, bh.timestamp))
                    .collect();
                delegated_messages.append(&mut messages);
            }
        }

        Ok(delegated_messages)
    }
}

fn filter_lowest_index(values: Vec<(EthHash, Cid, u64, usize)>) -> Vec<(EthHash, Cid, u64)> {
    let map: HashMap<EthHash, (Cid, u64, usize)> = values.into_iter().fold(
        HashMap::default(),
        |mut acc, (hash, cid, timestamp, index)| {
            acc.entry(hash)
                .and_modify(|&mut (_, _, ref mut min_index)| {
                    if index < *min_index {
                        *min_index = index;
                    }
                })
                .or_insert((cid, timestamp, index));
            acc
        },
    );

    map.into_iter()
        .map(|(hash, (cid, timestamp, _))| (hash, cid, timestamp))
        .collect()
}

/// Returns a Tuple of BLS messages of type `UnsignedMessage` and SECP messages
/// of type `SignedMessage`
pub fn block_messages<DB>(
    db: &DB,
    bh: &CachingBlockHeader,
) -> Result<(Vec<Message>, Vec<SignedMessage>), Error>
where
    DB: Blockstore,
{
    let (bls_cids, secpk_cids) = read_msg_cids(db, bh)?;

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
pub fn read_msg_cids<DB>(
    db: &DB,
    block_header: &CachingBlockHeader,
) -> Result<(Vec<Cid>, Vec<Cid>), Error>
where
    DB: Blockstore,
{
    let msg_cid = &block_header.messages;
    if let Some(roots) = db.get_cbor::<TxMeta>(msg_cid)? {
        let bls_cids = read_amt_cids(db, &roots.bls_message_root)?;
        let secpk_cids = read_amt_cids(db, &roots.secp_message_root)?;
        Ok((bls_cids, secpk_cids))
    } else {
        Err(Error::UndefinedKey(format!(
            "no msg root with cid {msg_cid} at epoch {} in block {}",
            block_header.epoch,
            block_header.cid(),
        )))
    }
}

/// Persists slice of `serializable` objects to `blockstore`.
pub fn persist_objects<'a, DB, C>(
    db: &DB,
    headers: impl Iterator<Item = &'a C>,
) -> Result<(), Error>
where
    DB: Blockstore,
    C: 'a + Serialize,
{
    for chunk in &headers.chunks(256) {
        db.bulk_put(chunk, DB::default_code())?;
    }
    Ok(())
}

/// Returns a vector of CIDs from provided root CID
fn read_amt_cids<DB>(db: &DB, root: &Cid) -> Result<Vec<Cid>, Error>
where
    DB: Blockstore,
{
    let amt = Amt::<Cid, _>::load(root, db)?;

    let mut cids = Vec::with_capacity(amt.count() as usize);
    amt.for_each_cacheless(|_, c| {
        cids.push(*c);
        Ok(())
    })?;

    Ok(cids)
}

/// Attempts to de-serialize to unsigned message or signed message and then
/// returns it as a [`ChainMessage`].
pub fn get_chain_message<DB>(db: &DB, key: &Cid) -> Result<ChainMessage, Error>
where
    DB: Blockstore,
{
    db.get_cbor(key)?
        .ok_or_else(|| Error::UndefinedKey(key.to_string()))
}

/// A cache structure mapping tipset keys to messages. The regular [`messages_for_tipset`], based
/// on performance measurements, is resource-intensive and can be a bottleneck for certain
/// use-cases. This cache is intended to be used with a complementary function;
/// [`messages_for_tipset_with_cache`].
pub struct MsgsInTipsetCache {
    cache: SizeTrackingLruCache<TipsetKey, Vec<ChainMessage>>,
}

impl MsgsInTipsetCache {
    pub fn new(capacity: NonZeroUsize) -> Self {
        Self {
            cache: SizeTrackingLruCache::new_with_metrics("msg_in_tipset".into(), capacity),
        }
    }

    pub fn get(&self, key: &TipsetKey) -> Option<Vec<ChainMessage>> {
        self.cache.get_cloned(key)
    }

    pub fn get_or_insert_with<F>(&self, key: &TipsetKey, f: F) -> anyhow::Result<Vec<ChainMessage>>
    where
        F: FnOnce() -> anyhow::Result<Vec<ChainMessage>>,
    {
        if self.cache.contains(key) {
            Ok(self.get(key).expect("cache entry disappeared!"))
        } else {
            let v = f()?;
            self.insert(key.clone(), v.clone());
            Ok(v)
        }
    }

    pub fn insert(&self, key: TipsetKey, mut value: Vec<ChainMessage>) {
        value.shrink_to_fit();
        self.cache.push(key, value);
    }

    /// Reads the intended cache size for this process from the environment or uses the default.
    fn read_cache_size() -> NonZeroUsize {
        // Arbitrary number, can be adjusted
        const DEFAULT: NonZeroUsize = nonzero!(100usize);
        std::env::var("FOREST_MESSAGES_IN_TIPSET_CACHE_SIZE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT)
    }
}

impl Default for MsgsInTipsetCache {
    fn default() -> Self {
        Self::new(Self::read_cache_size())
    }
}

/// Same as [`messages_for_tipset`] but uses a cache to store messages for each tipset.
pub fn messages_for_tipset_with_cache<DB>(
    db: &Arc<DB>,
    ts: &Tipset,
    cache: &MsgsInTipsetCache,
) -> Result<Vec<ChainMessage>, Error>
where
    DB: Blockstore,
{
    let key = ts.key();
    cache
        .get_or_insert_with(key, || {
            messages_for_tipset(db, ts).context("failed to get messages for tipset")
        })
        .map_err(Into::into)
}

/// Given a tipset this function will return all unique messages in that tipset.
/// Note: This function is resource-intensive and can be a bottleneck for certain use-cases.
/// Consider using [`messages_for_tipset_with_cache`] for better performance.
pub fn messages_for_tipset<DB>(db: &Arc<DB>, ts: &Tipset) -> Result<Vec<ChainMessage>, Error>
where
    DB: Blockstore,
{
    let mut applied: HashMap<Address, u64> = HashMap::new();
    let mut balances: HashMap<Address, TokenAmount> = HashMap::new();
    let state = StateTree::new_from_tipset(Arc::clone(db), ts)?;

    // message to get all messages for block_header into a single iterator
    let mut get_message_for_block_header =
        |b: &CachingBlockHeader| -> Result<Vec<ChainMessage>, Error> {
            let (unsigned, signed) = block_messages(db, b)?;
            let mut messages = Vec::with_capacity(unsigned.len() + signed.len());
            let unsigned_box = unsigned.into_iter().map(ChainMessage::Unsigned);
            let signed_box = signed.into_iter().map(ChainMessage::Signed);

            for message in unsigned_box.chain(signed_box) {
                let from_address = &message.from();
                if !applied.contains_key(from_address) {
                    let actor_state = state
                        .get_actor(from_address)?
                        .ok_or_else(|| Error::Other("Actor state not found".to_string()))?;
                    applied.insert(*from_address, actor_state.sequence);
                    balances.insert(*from_address, actor_state.balance.clone().into());
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

    ts.block_headers()
        .iter()
        .try_fold(Vec::new(), |mut message_vec, b| {
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
    keys.iter().map(|k| message_from_cid(db, k)).collect()
}

/// Returns message from key-value store based on a [`Cid`].
pub fn message_from_cid<DB, T>(db: &DB, key: &Cid) -> Result<T, Error>
where
    DB: Blockstore,
    T: DeserializeOwned,
{
    db.get_cbor(key)?
        .ok_or_else(|| Error::UndefinedKey(key.to_string()))
}

/// Returns parent message receipt given `block_header` and message index.
pub fn get_parent_receipt(
    db: &impl Blockstore,
    block_header: &CachingBlockHeader,
    i: usize,
) -> Result<Option<Receipt>, Error> {
    Ok(Receipt::get_receipt(
        db,
        &block_header.message_receipts,
        i as u64,
    )?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::multihash::prelude::*;
    use crate::{blocks::RawBlockHeader, shim::address::Address};
    use cid::Cid;
    use fvm_ipld_encoding::DAG_CBOR;

    #[test]
    fn genesis_test() {
        let db = Arc::new(crate::db::MemoryDB::default());
        let chain_config = Arc::new(ChainConfig::default());

        let gen_block = CachingBlockHeader::new(RawBlockHeader {
            miner_address: Address::new_id(0),
            state_root: Cid::new_v1(DAG_CBOR, MultihashCode::Identity.digest(&[])),
            epoch: 1,
            weight: 2u32.into(),
            messages: Cid::new_v1(DAG_CBOR, MultihashCode::Identity.digest(&[])),
            message_receipts: Cid::new_v1(DAG_CBOR, MultihashCode::Identity.digest(&[])),
            ..Default::default()
        });
        let cs =
            ChainStore::new(db.clone(), db.clone(), db, chain_config, gen_block.clone()).unwrap();

        assert_eq!(cs.genesis_block_header(), &gen_block);
    }

    #[test]
    fn block_validation_cache_basic() {
        let db = Arc::new(crate::db::MemoryDB::default());
        let chain_config = Arc::new(ChainConfig::default());
        let gen_block = CachingBlockHeader::new(RawBlockHeader {
            miner_address: Address::new_id(0),
            ..Default::default()
        });

        let cs = ChainStore::new(db.clone(), db.clone(), db, chain_config, gen_block).unwrap();

        let cid = Cid::new_v1(DAG_CBOR, MultihashCode::Blake2b256.digest(&[1, 2, 3]));
        assert!(!cs.is_block_validated(&cid));

        cs.mark_block_as_validated(&cid);
        assert!(cs.is_block_validated(&cid));
    }

    #[test]
    fn test_messages_in_tipset_cache() {
        let cache = MsgsInTipsetCache::new(2.try_into().unwrap());
        let key1 = TipsetKey::from(nunny::vec![Cid::new_v1(
            DAG_CBOR,
            MultihashCode::Blake2b256.digest(&[1])
        )]);
        assert!(cache.get(&key1).is_none());

        let msgs = vec![ChainMessage::Unsigned(Message::default())];
        cache.insert(key1.clone(), msgs.clone());
        assert_eq!(msgs, cache.get(&key1).unwrap());

        let inserter_executed: std::sync::atomic::AtomicBool =
            std::sync::atomic::AtomicBool::new(false);
        let key_inserter = || {
            inserter_executed.store(true, std::sync::atomic::Ordering::Relaxed);
            Ok(msgs.clone())
        };

        assert_eq!(msgs, cache.get_or_insert_with(&key1, key_inserter).unwrap());
        assert!(!inserter_executed.load(std::sync::atomic::Ordering::Relaxed));

        let key2 = TipsetKey::from(nunny::vec![Cid::new_v1(
            DAG_CBOR,
            MultihashCode::Blake2b256.digest(&[2])
        )]);

        assert!(cache.get(&key2).is_none());
        assert_eq!(msgs, cache.get_or_insert_with(&key2, key_inserter).unwrap());
        assert!(inserter_executed.load(std::sync::atomic::Ordering::Relaxed));
    }
}
