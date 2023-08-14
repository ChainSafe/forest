// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::sync::Arc;

use crate::blocks::{BlockHeader, Tipset, TipsetKeys, TxMeta};
use crate::interpreter::BlockMessages;
use crate::ipld::FrozenCids;
use crate::libp2p_bitswap::{BitswapStoreRead, BitswapStoreReadWrite};
use crate::message::{ChainMessage, Message as MessageTrait, SignedMessage};
use crate::networks::ChainConfig;
use crate::shim::clock::ChainEpoch;
use crate::shim::{
    address::Address, econ::TokenAmount, executor::Receipt, message::Message,
    state_tree::StateTree, version::NetworkVersion,
};
use crate::utils::db::{BlockstoreExt, CborStoreExt};
use ahash::{HashMap, HashMapExt, HashSet};
use anyhow::Result;
use cid::Cid;
use fvm_ipld_amt::Amtv0 as Amt;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::CborStore;
use parking_lot::Mutex;
use serde::{de::DeserializeOwned, Serialize};
use tokio::sync::broadcast::{self, Sender as Publisher};
use tracing::{debug, info, warn};

use super::{
    index::{ChainIndex, ResolveNullTipset},
    tipset_tracker::TipsetTracker,
    Error,
};
use crate::chain::Scale;
use crate::db::setting_keys::{ESTIMATED_RECORDS_KEY, HEAD_KEY};
use crate::db::{SettingsStore, SettingsStoreExt};

// A cap on the size of the future_sink
const SINK_CAP: usize = 200;

/// Disambiguate the type to signify that we are expecting a delta and not an actual epoch/height
/// while maintaining the same type.
pub type ChainEpochDelta = ChainEpoch;

/// `Enum` for `pubsub` channel that defines message type variant and data
/// contained in message type.
#[derive(Clone, Debug)]
pub enum HeadChange {
    Apply(Arc<Tipset>),
}

/// Stores chain data such as heaviest tipset and cached tipset info at each
/// epoch. This structure is thread-safe, and all caches are wrapped in a mutex
/// to allow a consistent `ChainStore` to be shared across tasks.
pub struct ChainStore<DB> {
    /// Publisher for head change events
    publisher: Publisher<HeadChange>,

    /// key-value `datastore`.
    pub db: Arc<DB>,

    /// Settings store
    settings: Arc<dyn SettingsStore + Sync + Send>,

    /// Used as a cache for tipset `lookbacks`.
    pub chain_index: Arc<ChainIndex<Arc<DB>>>,

    /// Tracks blocks for the purpose of forming tipsets.
    tipset_tracker: TipsetTracker<DB>,

    genesis_block_header: BlockHeader,

    /// validated blocks
    validated_blocks: Mutex<HashSet<Cid>>,
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
    type Params = <DB as BitswapStoreReadWrite>::Params;

    fn insert(&self, block: &libipld::Block<Self::Params>) -> anyhow::Result<()> {
        self.db.insert(block)
    }
}

impl<DB> ChainStore<DB>
where
    DB: Blockstore,
{
    pub fn new(
        db: Arc<DB>,
        settings: Arc<dyn SettingsStore + Sync + Send>,
        chain_config: Arc<ChainConfig>,
        genesis_block_header: BlockHeader,
    ) -> Result<Self> {
        let (publisher, _) = broadcast::channel(SINK_CAP);
        let chain_index = Arc::new(ChainIndex::new(Arc::clone(&db)));

        if !settings
            .read_obj::<TipsetKeys>(HEAD_KEY)?
            .is_some_and(|tipset_keys| chain_index.load_tipset(&tipset_keys).is_ok())
        {
            let tipset_keys = TipsetKeys::new(FrozenCids::from_iter([*genesis_block_header.cid()]));
            settings.write_obj(HEAD_KEY, &tipset_keys)?;
        }

        let validated_blocks = Mutex::new(HashSet::default());

        let cs = Self {
            publisher,
            chain_index,
            tipset_tracker: TipsetTracker::new(Arc::clone(&db), chain_config),
            db,
            settings,
            genesis_block_header,
            validated_blocks,
        };

        Ok(cs)
    }

    /// Sets heaviest tipset within `ChainStore` and store its tipset keys in
    /// the settings store under the [`crate::db::setting_keys::HEAD_KEY`] key.
    pub fn set_heaviest_tipset(&self, ts: Arc<Tipset>) -> Result<(), Error> {
        self.settings.write_obj(HEAD_KEY, ts.key())?;
        if self.publisher.send(HeadChange::Apply(ts)).is_err() {
            debug!("did not publish head change, no active receivers");
        }
        Ok(())
    }

    /// Adds a [`BlockHeader`] to the tipset tracker, which tracks valid
    /// headers.
    pub fn add_to_tipset_tracker(&self, header: &BlockHeader) {
        self.tipset_tracker.add(header);
    }

    pub fn set_estimated_records(&self, records: u64) -> anyhow::Result<()> {
        self.settings.write_obj(ESTIMATED_RECORDS_KEY, &records)?;
        Ok(())
    }

    /// Writes tipset block headers to data store and updates heaviest tipset
    /// with other compatible tracked headers.
    pub fn put_tipset<S>(&self, ts: &Tipset) -> Result<(), Error>
    where
        S: Scale,
    {
        // TODO: we could add the blocks of `ts` to the tipset tracker from here,
        // making `add_to_tipset_tracker` redundant and decreasing the number of
        // `blockstore` reads
        persist_objects(self.blockstore(), ts.blocks())?;

        // Expand tipset to include other compatible blocks at the epoch.
        let expanded = self.expand_tipset(ts.min_ticket_block().clone())?;
        self.update_heaviest::<S>(Arc::new(expanded))?;
        Ok(())
    }

    /// Expands tipset to tipset with all other headers in the same epoch using
    /// the tipset tracker.
    fn expand_tipset(&self, header: BlockHeader) -> Result<Tipset, Error> {
        self.tipset_tracker.expand(header)
    }

    /// Returns genesis [`BlockHeader`].
    pub fn genesis(&self) -> &BlockHeader {
        &self.genesis_block_header
    }

    /// Returns the currently tracked heaviest tipset.
    pub fn heaviest_tipset(&self) -> Arc<Tipset> {
        self.tipset_from_keys(
            &self
                .settings
                .require_obj::<TipsetKeys>(HEAD_KEY)
                .expect("failed to load heaviest tipset"),
        )
        .expect("failed to load heaviest tipset")
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
    #[tracing::instrument(skip_all)]
    pub fn tipset_from_keys(&self, tsk: &TipsetKeys) -> Result<Arc<Tipset>, Error> {
        if tsk.cids.is_empty() {
            return Ok(self.heaviest_tipset());
        }
        self.chain_index.load_tipset(tsk)
    }

    /// Determines if provided tipset is heavier than existing known heaviest
    /// tipset
    fn update_heaviest<S>(&self, ts: Arc<Tipset>) -> Result<(), Error>
    where
        S: Scale,
    {
        // Calculate heaviest weight before matching to avoid deadlock with mutex
        let heaviest_weight = S::weight(&self.db, &self.heaviest_tipset())?;

        let new_weight = S::weight(&self.db, ts.as_ref())?;
        let curr_weight = heaviest_weight;

        if new_weight > curr_weight {
            // TODO potentially need to deal with re-orgs here
            info!("New heaviest tipset! {} (EPOCH = {})", ts.key(), ts.epoch());
            self.set_heaviest_tipset(ts)?;
        }
        Ok(())
    }

    /// Checks metadata file if block has already been validated.
    pub fn is_block_validated(&self, cid: &Cid) -> bool {
        let validated = self.validated_blocks.lock().contains(cid);
        if validated {
            debug!("Block {cid} was previously validated");
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

    // FIXME: This function doesn't use the chain store at all.
    //        Tracking issue: https://github.com/ChainSafe/forest/issues/3208
    /// Retrieves block messages to be passed through the VM.
    ///
    /// It removes duplicate messages which appear in multiple blocks.
    pub fn block_msgs_for_tipset(db: DB, ts: &Tipset) -> Result<Vec<BlockMessages>, Error> {
        let mut applied = HashMap::new();
        let mut select_msg = |m: ChainMessage| -> Option<ChainMessage> {
            // The first match for a sender is guaranteed to have correct nonce
            // the block isn't valid otherwise.
            let entry = applied.entry(m.from()).or_insert_with(|| m.sequence());

            if *entry != m.sequence() {
                return None;
            }

            *entry += 1;
            Some(m)
        };

        ts.blocks()
            .iter()
            .map(|b| {
                let (usm, sm) = block_messages(&db, b)?;

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

    /// Retrieves ordered valid messages from a `Tipset`. This will only include
    /// messages that will be passed through the VM.
    pub fn messages_for_tipset(&self, ts: &Tipset) -> Result<Vec<ChainMessage>, Error> {
        let bmsgs = ChainStore::block_msgs_for_tipset(&self.db, ts)?;
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
        chain_index: Arc<ChainIndex<Arc<DB>>>,
        chain_config: Arc<ChainConfig>,
        heaviest_tipset: Arc<Tipset>,
        round: ChainEpoch,
    ) -> Result<(Arc<Tipset>, Cid), Error>
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
            let genesis_timestamp = heaviest_tipset.genesis(&chain_index.db)?.timestamp();
            let beacon = Arc::new(chain_config.get_beacon_schedule(genesis_timestamp));
            let (state, _) = crate::state_manager::apply_block_messages(
                genesis_timestamp,
                Arc::clone(&chain_index),
                Arc::clone(&chain_config),
                beacon,
                // Creating new WASM engines is expensive (takes seconds to
                // minutes). It's only acceptable here because this situation is
                // so rare (may happen in dev-networks, doesn't happen in
                // calibnet or mainnet.)
                &crate::shim::machine::MultiEngine::default(),
                Arc::clone(&heaviest_tipset),
                crate::state_manager::NO_CALLBACK,
            )
            .map_err(|e| Error::Other(e.to_string()))?;
            return Ok((heaviest_tipset, state));
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
            .load_tipset(next_ts.parents())
            .map_err(|e| Error::Other(format!("Could not get tipset from keys {e:?}")))?;
        Ok((lbts, *next_ts.parent_state()))
    }
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
    if let Some(roots) = db.get_cbor::<TxMeta>(msg_cid)? {
        let bls_cids = read_amt_cids(db, &roots.bls_message_root)?;
        let secpk_cids = read_amt_cids(db, &roots.secp_message_root)?;
        Ok((bls_cids, secpk_cids))
    } else {
        Err(Error::UndefinedKey(format!(
            "no msg root with cid {msg_cid}"
        )))
    }
}

/// Persists slice of `serializable` objects to `blockstore`.
pub fn persist_objects<DB, C>(db: &DB, headers: &[C]) -> Result<(), Error>
where
    DB: Blockstore,
    C: Serialize,
{
    for chunk in headers.chunks(256) {
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

    let mut cids = Vec::new();
    for i in 0..amt.count() {
        if let Some(c) = amt.get(i)? {
            cids.push(*c);
        }
    }

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

/// Given a tipset this function will return all unique messages in that tipset.
pub fn messages_for_tipset<DB>(db: Arc<DB>, ts: &Tipset) -> Result<Vec<ChainMessage>, Error>
where
    DB: Blockstore,
{
    let mut applied: HashMap<Address, u64> = HashMap::new();
    let mut balances: HashMap<Address, TokenAmount> = HashMap::new();
    let state = StateTree::new_from_root(Arc::clone(&db), ts.parent_state())?;

    // message to get all messages for block_header into a single iterator
    let mut get_message_for_block_header = |b: &BlockHeader| -> Result<Vec<ChainMessage>, Error> {
        let (unsigned, signed) = block_messages(&db, b)?;
        let mut messages = Vec::with_capacity(unsigned.len() + signed.len());
        let unsigned_box = unsigned.into_iter().map(ChainMessage::Unsigned);
        let signed_box = signed.into_iter().map(ChainMessage::Signed);

        for message in unsigned_box.chain(signed_box) {
            let from_address = &message.from();
            if applied.contains_key(from_address) {
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
            db.get_cbor(k)?
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
    let receipts = amt.get(i as u64)?;
    Ok(receipts.cloned())
}

pub mod headchange_json {
    use crate::blocks::tipset_json::TipsetJson;
    use serde::{Deserialize, Serialize};

    use super::*;

    #[derive(Debug, Deserialize, Serialize)]
    #[serde(rename_all = "lowercase")]
    #[serde(tag = "type", content = "val")]
    pub enum HeadChangeJson {
        Current(TipsetJson),
        Apply(TipsetJson),
        Revert(TipsetJson),
    }

    impl From<HeadChange> for HeadChangeJson {
        fn from(wrapper: HeadChange) -> Self {
            match wrapper {
                HeadChange::Apply(tipset) => HeadChangeJson::Apply(TipsetJson(tipset)),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::shim::address::Address;
    use cid::{
        multihash::{
            Code::{Blake2b256, Identity},
            MultihashDigest,
        },
        Cid,
    };
    use fvm_ipld_encoding::DAG_CBOR;

    use super::*;

    #[test]
    fn genesis_test() {
        let db = Arc::new(crate::db::MemoryDB::default());
        let chain_config = Arc::new(ChainConfig::default());

        let gen_block = BlockHeader::builder()
            .epoch(1)
            .weight(2_u32.into())
            .messages(Cid::new_v1(DAG_CBOR, Identity.digest(&[])))
            .message_receipts(Cid::new_v1(DAG_CBOR, Identity.digest(&[])))
            .state_root(Cid::new_v1(DAG_CBOR, Identity.digest(&[])))
            .miner_address(Address::new_id(0))
            .build()
            .unwrap();
        let cs = ChainStore::new(db.clone(), db, chain_config, gen_block.clone()).unwrap();

        assert_eq!(cs.genesis(), &gen_block);
    }

    #[test]
    fn block_validation_cache_basic() {
        let db = Arc::new(crate::db::MemoryDB::default());
        let chain_config = Arc::new(ChainConfig::default());
        let gen_block = BlockHeader::builder()
            .miner_address(Address::new_id(0))
            .build()
            .unwrap();

        let cs = ChainStore::new(db.clone(), db, chain_config, gen_block).unwrap();

        let cid = Cid::new_v1(DAG_CBOR, Blake2b256.digest(&[1, 2, 3]));
        assert!(!cs.is_block_validated(&cid));

        cs.mark_block_as_validated(&cid);
        assert!(cs.is_block_validated(&cid));
    }
}
