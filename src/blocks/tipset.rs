// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{
    fmt,
    sync::{Arc, OnceLock},
};

use super::{Block, CachingBlockHeader, RawBlockHeader, Ticket};
use crate::{
    chain_sync::TipsetValidator,
    cid_collections::SmallCidNonEmptyVec,
    networks::{calibnet, mainnet},
    shim::clock::ChainEpoch,
    utils::{cid::CidCborExt, get_size::nunny_vec_heap_size_helper},
};
use ahash::HashMap;
use anyhow::Context as _;
use cid::Cid;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::CborStore;
use get_size2::GetSize;
use itertools::Itertools as _;
use num::BigInt;
use nunny::{Vec as NonEmpty, vec as nonempty};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// A set of `CIDs` forming a unique key for a Tipset.
/// Equal keys will have equivalent iteration order, but note that the `CIDs`
/// are *not* maintained in the same order as the canonical iteration order of
/// blocks in a tipset (which is by ticket)
#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    PartialOrd,
    Ord,
    GetSize,
    derive_more::IntoIterator,
)]
#[cfg_attr(test, derive(derive_quickcheck_arbitrary::Arbitrary))]
pub struct TipsetKey(#[into_iterator(owned, ref)] SmallCidNonEmptyVec);

impl TipsetKey {
    // Special encoding to match Lotus.
    pub fn cid(&self) -> anyhow::Result<Cid> {
        use fvm_ipld_encoding::RawBytes;

        let mut bytes = Vec::new();
        for cid in self.to_cids() {
            bytes.append(&mut cid.to_bytes())
        }
        Ok(Cid::from_cbor_blake2b256(&RawBytes::new(bytes))?)
    }

    /// Returns `true` if the tipset key contains the given CID.
    pub fn contains(&self, cid: Cid) -> bool {
        self.0.contains(cid)
    }

    /// Returns a non-empty collection of `CID`
    pub fn into_cids(self) -> NonEmpty<Cid> {
        self.0.into_cids()
    }

    /// Returns a non-empty collection of `CID`
    pub fn to_cids(&self) -> NonEmpty<Cid> {
        self.0.clone().into_cids()
    }

    /// Returns an iterator of `CID`s.
    pub fn iter(&self) -> impl Iterator<Item = Cid> + '_ {
        self.0.iter()
    }

    /// Returns the number of `CID`s
    pub fn len(&self) -> usize {
        self.0.len()
    }

    // To suppress `#[warn(clippy::len_without_is_empty)]`
    pub fn is_empty(&self) -> bool {
        false
    }

    /// Terse representation of the tipset key.
    /// `bafy2bzaceaqrqoasufr7gdwrbhvlfy2xmc4e5sdzekjgyha2kldxigu73gilo`
    /// becomes `eaq...ilo`. The `bafy2bzac` prefix is removed.
    pub fn terse(&self) -> String {
        fn terse_cid(cid: Cid) -> String {
            let s = cid::multibase::encode(
                cid::multibase::Base::Base32Lower,
                cid.to_bytes().as_slice(),
            );
            format!("{}...{}", &s[9..12], &s[s.len() - 3..])
        }
        self.to_cids()
            .into_iter()
            .map(terse_cid)
            .collect_vec()
            .join(", ")
    }

    /// Formats tipset key to match the Lotus display.
    pub fn format_lotus(&self) -> String {
        format!("{{{}}}", self.to_cids().into_iter().join(","))
    }
}

impl From<NonEmpty<Cid>> for TipsetKey {
    fn from(mut value: NonEmpty<Cid>) -> Self {
        // When `value.capacity() > value.len()`, it takes more heap memory.
        // Always shrink it since `TipsetKey` is immutable and used in caches.
        value.shrink_to_fit();
        Self(value.into())
    }
}

impl fmt::Display for TipsetKey {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let s = self
            .to_cids()
            .into_iter()
            .map(|cid| cid.to_string())
            .collect_vec()
            .join(", ");
        write!(f, "[{s}]")
    }
}

#[cfg(test)]
impl Default for TipsetKey {
    fn default() -> Self {
        nunny::vec![Cid::default()].into()
    }
}

/// An immutable set of blocks at the same height with the same parent set.
/// Blocks in a tipset are canonically ordered by ticket size.
///
/// Represents non-null tipsets, see the documentation on [`crate::state_manager::apply_block_messages`]
/// for more.
#[derive(Clone, Debug, GetSize)]
pub struct Tipset {
    /// Sorted
    #[get_size(size_fn = nunny_vec_heap_size_helper)]
    headers: Arc<NonEmpty<CachingBlockHeader>>,
    // key is lazily initialized via `fn key()`.
    key: Arc<OnceLock<TipsetKey>>,
}

impl From<RawBlockHeader> for Tipset {
    fn from(value: RawBlockHeader) -> Self {
        Self::from(CachingBlockHeader::from(value))
    }
}

impl From<&CachingBlockHeader> for Tipset {
    fn from(value: &CachingBlockHeader) -> Self {
        value.clone().into()
    }
}

impl From<CachingBlockHeader> for Tipset {
    fn from(value: CachingBlockHeader) -> Self {
        Self {
            headers: nonempty![value].into(),
            key: OnceLock::new().into(),
        }
    }
}

impl From<NonEmpty<CachingBlockHeader>> for Tipset {
    fn from(headers: NonEmpty<CachingBlockHeader>) -> Self {
        Self {
            headers: headers.into(),
            key: OnceLock::new().into(),
        }
    }
}

impl PartialEq for Tipset {
    fn eq(&self, other: &Self) -> bool {
        self.headers.eq(&other.headers)
    }
}

#[cfg(test)]
impl quickcheck::Arbitrary for Tipset {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        // TODO(forest): https://github.com/ChainSafe/forest/issues/3570
        //               Support random generation of tipsets with multiple blocks.
        Tipset::from(CachingBlockHeader::arbitrary(g))
    }
}

impl From<FullTipset> for Tipset {
    fn from(FullTipset { key, blocks }: FullTipset) -> Self {
        let headers = Arc::unwrap_or_clone(blocks)
            .into_iter_ne()
            .map(|block| block.header)
            .collect_vec()
            .into();
        Tipset { headers, key }
    }
}

#[derive(Error, Debug, PartialEq)]
pub enum CreateTipsetError {
    #[error("tipsets must not be empty")]
    Empty,
    #[error(
        "parent CID is inconsistent. All block headers in a tipset must agree on their parent tipset"
    )]
    BadParents,
    #[error(
        "state root is inconsistent. All block headers in a tipset must agree on their parent state root"
    )]
    BadStateRoot,
    #[error("epoch is inconsistent. All block headers in a tipset must agree on their epoch")]
    BadEpoch,
    #[error("duplicate miner address. All miners in a tipset must be unique.")]
    DuplicateMiner,
}

#[allow(clippy::len_without_is_empty)]
impl Tipset {
    /// Builds a new Tipset from a collection of blocks.
    /// A valid tipset contains a non-empty collection of blocks that have
    /// distinct miners and all specify identical epoch, parents, weight,
    /// height, state root, receipt root; content-id for headers are
    /// supposed to be distinct but until encoding is added will be equal.
    pub fn new<H: Into<CachingBlockHeader>>(
        headers: impl IntoIterator<Item = H>,
    ) -> Result<Self, CreateTipsetError> {
        let mut headers = NonEmpty::new(
            headers
                .into_iter()
                .map(Into::<CachingBlockHeader>::into)
                .sorted_by_cached_key(|it| it.tipset_sort_key())
                .collect(),
        )
        .map_err(|_| CreateTipsetError::Empty)?;
        headers.shrink_to_fit();
        verify_block_headers(&headers)?;

        Ok(Self {
            headers: headers.into(),
            key: OnceLock::new().into(),
        })
    }

    /// Fetch a tipset from the blockstore. This call fails if the tipset is
    /// present but invalid. If the tipset is missing, None is returned.
    pub fn load(store: &impl Blockstore, tsk: &TipsetKey) -> anyhow::Result<Option<Tipset>> {
        Ok(tsk
            .to_cids()
            .into_iter()
            .map(|key| CachingBlockHeader::load(store, key))
            .collect::<anyhow::Result<Option<Vec<_>>>>()?
            .map(Tipset::new)
            .transpose()?)
    }

    /// Fetch a tipset from the blockstore. This calls fails if the tipset is
    /// missing or invalid.
    pub fn load_required(store: &impl Blockstore, tsk: &TipsetKey) -> anyhow::Result<Tipset> {
        Tipset::load(store, tsk)?.context("Required tipset missing from database")
    }

    /// Returns epoch of the tipset.
    pub fn epoch(&self) -> ChainEpoch {
        self.min_ticket_block().epoch
    }
    pub fn block_headers(&self) -> &NonEmpty<CachingBlockHeader> {
        &self.headers
    }
    /// Returns the smallest ticket of all blocks in the tipset
    pub fn min_ticket(&self) -> Option<&Ticket> {
        self.min_ticket_block().ticket.as_ref()
    }
    /// Returns the block with the smallest ticket of all blocks in the tipset
    pub fn min_ticket_block(&self) -> &CachingBlockHeader {
        self.headers.first()
    }
    /// Returns the smallest timestamp of all blocks in the tipset
    pub fn min_timestamp(&self) -> u64 {
        self.headers
            .iter()
            .map(|block| block.timestamp)
            .min()
            .unwrap()
    }
    /// Returns the number of blocks in the tipset.
    pub fn len(&self) -> usize {
        self.headers.len()
    }
    /// Returns a key for the tipset.
    pub fn key(&self) -> &TipsetKey {
        self.key
            .get_or_init(|| TipsetKey::from(self.headers.iter_ne().map(|h| *h.cid()).collect_vec()))
    }
    /// Returns a non-empty collection of `CIDs` for the current tipset
    pub fn cids(&self) -> NonEmpty<Cid> {
        self.key().to_cids()
    }
    /// Returns the keys of the parents of the blocks in the tipset.
    pub fn parents(&self) -> &TipsetKey {
        &self.min_ticket_block().parents
    }
    /// Returns the state root for the tipset parent.
    pub fn parent_state(&self) -> &Cid {
        &self.min_ticket_block().state_root
    }
    /// Returns the message receipt root for the tipset parent.
    pub fn parent_message_receipts(&self) -> &Cid {
        &self.min_ticket_block().message_receipts
    }
    /// Returns the tipset's calculated weight
    pub fn weight(&self) -> &BigInt {
        &self.min_ticket_block().weight
    }
    /// Returns true if self wins according to the Filecoin tie-break rule
    /// (FIP-0023)
    #[cfg(test)]
    pub fn break_weight_tie(&self, other: &Tipset) -> bool {
        // blocks are already sorted by ticket
        let broken = self
            .block_headers()
            .iter()
            .zip(other.block_headers().iter())
            .any(|(a, b)| {
                const MSG: &str =
                    "The function block_sanity_checks should have been called at this point.";
                let ticket = a.ticket.as_ref().expect(MSG);
                let other_ticket = b.ticket.as_ref().expect(MSG);
                ticket.vrfproof < other_ticket.vrfproof
            });
        if broken {
            tracing::info!("Weight tie broken in favour of {}", self.key());
        } else {
            tracing::info!("Weight tie left unbroken, default to {}", other.key());
        }
        broken
    }

    /// Returns an iterator of all tipsets, taking an owned [`Blockstore`]
    pub fn chain_owned(self, store: impl Blockstore) -> impl Iterator<Item = Tipset> {
        let mut tipset = Some(self);
        std::iter::from_fn(move || {
            let child = tipset.take()?;
            tipset = Tipset::load_required(&store, child.parents()).ok();
            Some(child)
        })
    }

    /// Returns an iterator of all tipsets
    pub fn chain(self, store: &impl Blockstore) -> impl Iterator<Item = Tipset> + '_ {
        let mut tipset = Some(self);
        std::iter::from_fn(move || {
            let child = tipset.take()?;
            tipset = Tipset::load_required(store, child.parents()).ok();
            Some(child)
        })
    }

    /// Fetch the genesis block header for a given tipset.
    pub fn genesis(&self, store: &impl Blockstore) -> anyhow::Result<CachingBlockHeader> {
        // Scanning through millions of epochs to find the genesis is quite
        // slow. Let's use a list of known blocks to short-circuit the search.
        // The blocks are hash-chained together and known blocks are guaranteed
        // to have a known genesis.
        #[derive(Serialize, Deserialize)]
        struct KnownHeaders {
            calibnet: HashMap<ChainEpoch, String>,
            mainnet: HashMap<ChainEpoch, String>,
        }

        static KNOWN_HEADERS: OnceLock<KnownHeaders> = OnceLock::new();
        let headers = KNOWN_HEADERS.get_or_init(|| {
            serde_yaml::from_str(include_str!("../../build/known_blocks.yaml")).unwrap()
        });

        for tipset in self.clone().chain(store) {
            // Search for known calibnet and mainnet blocks
            for (genesis_cid, known_blocks) in [
                (*calibnet::GENESIS_CID, &headers.calibnet),
                (*mainnet::GENESIS_CID, &headers.mainnet),
            ] {
                if let Some(known_block_cid) = known_blocks.get(&tipset.epoch())
                    && known_block_cid == &tipset.min_ticket_block().cid().to_string()
                {
                    return store
                        .get_cbor(&genesis_cid)?
                        .context("Genesis block missing from database");
                }
            }

            // If no known blocks are found, we'll eventually hit the genesis tipset.
            if tipset.epoch() == 0 {
                return Ok(tipset.min_ticket_block().clone());
            }
        }
        anyhow::bail!("Genesis block not found")
    }
}

/// `FullTipset` is an expanded version of a tipset that contains all the blocks
/// and messages.
#[derive(Debug, Clone, Eq)]
pub struct FullTipset {
    blocks: Arc<NonEmpty<Block>>,
    // key is lazily initialized via `fn key()`.
    key: Arc<OnceLock<TipsetKey>>,
}

impl std::hash::Hash for FullTipset {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.key().hash(state)
    }
}

// Constructing a FullTipset from a single Block is infallible.
impl From<Block> for FullTipset {
    fn from(block: Block) -> Self {
        FullTipset {
            blocks: nonempty![block].into(),
            key: OnceLock::new().into(),
        }
    }
}

impl PartialEq for FullTipset {
    fn eq(&self, other: &Self) -> bool {
        self.blocks.eq(&other.blocks)
    }
}

impl FullTipset {
    pub fn new(blocks: impl IntoIterator<Item = Block>) -> Result<Self, CreateTipsetError> {
        let blocks = Arc::new(
            NonEmpty::new(
                // sort blocks on creation to allow for more seamless conversions between
                // FullTipset and Tipset
                blocks
                    .into_iter()
                    .sorted_by_cached_key(|it| it.header.tipset_sort_key())
                    .collect(),
            )
            .map_err(|_| CreateTipsetError::Empty)?,
        );

        verify_block_headers(blocks.iter().map(|it| &it.header))?;

        Ok(Self {
            blocks,
            key: Arc::new(OnceLock::new()),
        })
    }
    /// Returns the first block of the tipset.
    fn first_block(&self) -> &Block {
        self.blocks.first()
    }
    /// Returns reference to all blocks in a full tipset.
    pub fn blocks(&self) -> &NonEmpty<Block> {
        &self.blocks
    }
    /// Returns all blocks in a full tipset.
    pub fn into_blocks(self) -> NonEmpty<Block> {
        Arc::unwrap_or_clone(self.blocks)
    }
    /// Converts the full tipset into a [Tipset] which removes the messages
    /// attached.
    pub fn into_tipset(self) -> Tipset {
        Tipset::from(self)
    }
    /// Returns a key for the tipset.
    pub fn key(&self) -> &TipsetKey {
        self.key
            .get_or_init(|| TipsetKey::from(self.blocks.iter_ne().map(|b| *b.cid()).collect_vec()))
    }
    /// Returns the state root for the tipset parent.
    pub fn parent_state(&self) -> &Cid {
        &self.first_block().header().state_root
    }
    /// Returns the keys of the parents of the blocks in the tipset.
    pub fn parents(&self) -> &TipsetKey {
        &self.first_block().header().parents
    }
    /// Returns epoch of the tipset.
    pub fn epoch(&self) -> ChainEpoch {
        self.first_block().header().epoch
    }
    /// Returns the tipset's calculated weight.
    pub fn weight(&self) -> &BigInt {
        &self.first_block().header().weight
    }
    /// Persists the tipset into the blockstore.
    pub fn persist(&self, db: &impl Blockstore) -> anyhow::Result<()> {
        for block in self.blocks() {
            // To persist `TxMeta` that is required for loading tipset messages
            TipsetValidator::validate_msg_root(db, block)?;
            crate::chain::persist_objects(&db, std::iter::once(block.header()))?;
            crate::chain::persist_objects(&db, block.bls_msgs().iter())?;
            crate::chain::persist_objects(&db, block.secp_msgs().iter())?;
        }
        Ok(())
    }
}

fn verify_block_headers<'a>(
    headers: impl IntoIterator<Item = &'a CachingBlockHeader>,
) -> Result<(), CreateTipsetError> {
    use itertools::all;

    let headers =
        NonEmpty::new(headers.into_iter().collect()).map_err(|_| CreateTipsetError::Empty)?;
    if !all(&headers, |it| it.parents == headers.first().parents) {
        return Err(CreateTipsetError::BadParents);
    }
    if !all(&headers, |it| it.state_root == headers.first().state_root) {
        return Err(CreateTipsetError::BadStateRoot);
    }
    if !all(&headers, |it| it.epoch == headers.first().epoch) {
        return Err(CreateTipsetError::BadEpoch);
    }

    if !headers.iter().map(|it| it.miner_address).all_unique() {
        return Err(CreateTipsetError::DuplicateMiner);
    }

    Ok(())
}

#[cfg_vis::cfg_vis(doc, pub)]
mod lotus_json {
    //! [Tipset] isn't just plain old data - it has an invariant (all block headers are valid)
    //! So there is custom de-serialization here

    use crate::blocks::{CachingBlockHeader, Tipset};
    use crate::lotus_json::*;
    use nunny::Vec as NonEmpty;
    use schemars::JsonSchema;
    use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};

    use super::TipsetKey;

    #[derive(Debug, PartialEq, Clone, JsonSchema)]
    #[schemars(rename = "Tipset")]
    pub struct TipsetLotusJson(#[schemars(with = "TipsetLotusJsonInner")] Tipset);

    #[derive(Serialize, Deserialize, JsonSchema)]
    #[schemars(rename = "TipsetInner")]
    #[serde(rename_all = "PascalCase")]
    struct TipsetLotusJsonInner {
        #[serde(with = "crate::lotus_json")]
        #[schemars(with = "LotusJson<TipsetKey>")]
        cids: TipsetKey,
        #[serde(with = "crate::lotus_json")]
        #[schemars(with = "LotusJson<NonEmpty<CachingBlockHeader>>")]
        blocks: NonEmpty<CachingBlockHeader>,
        height: i64,
    }

    impl<'de> Deserialize<'de> for TipsetLotusJson {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: Deserializer<'de>,
        {
            let TipsetLotusJsonInner {
                cids: _ignored0,
                blocks,
                height: _ignored1,
            } = Deserialize::deserialize(deserializer)?;

            Ok(Self(Tipset::new(blocks).map_err(D::Error::custom)?))
        }
    }

    impl Serialize for TipsetLotusJson {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            let Self(tipset) = self;
            TipsetLotusJsonInner {
                cids: tipset.key().clone(),
                height: tipset.epoch(),
                blocks: tipset.block_headers().clone(),
            }
            .serialize(serializer)
        }
    }

    impl HasLotusJson for Tipset {
        type LotusJson = TipsetLotusJson;

        #[cfg(test)]
        fn snapshots() -> Vec<(serde_json::Value, Self)> {
            use serde_json::json;
            vec![(
                json!({
                    "Blocks": [
                        {
                            "BeaconEntries": null,
                            "ForkSignaling": 0,
                            "Height": 0,
                            "Messages": { "/": "baeaaaaa" },
                            "Miner": "f00",
                            "ParentBaseFee": "0",
                            "ParentMessageReceipts": { "/": "baeaaaaa" },
                            "ParentStateRoot": { "/":"baeaaaaa" },
                            "ParentWeight": "0",
                            "Parents": [{"/":"bafyreiaqpwbbyjo4a42saasj36kkrpv4tsherf2e7bvezkert2a7dhonoi"}],
                            "Timestamp": 0,
                            "WinPoStProof": null
                        }
                    ],
                    "Cids": [
                        { "/": "bafy2bzaceag62hjj3o43lf6oyeox3fvg5aqkgl5zagbwpjje3ajwg6yw4iixk" }
                    ],
                    "Height": 0
                }),
                Self::new(vec![CachingBlockHeader::default()]).unwrap(),
            )]
        }

        fn into_lotus_json(self) -> Self::LotusJson {
            TipsetLotusJson(self)
        }

        fn from_lotus_json(TipsetLotusJson(tipset): Self::LotusJson) -> Self {
            tipset
        }
    }

    #[test]
    fn snapshots() {
        assert_all_snapshots::<Tipset>()
    }

    #[cfg(test)]
    quickcheck::quickcheck! {
        fn quickcheck(val: Tipset) -> () {
            assert_unchanged_via_json(val)
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::blocks::VRFProof;
    use crate::blocks::{
        CachingBlockHeader, ElectionProof, Ticket, Tipset, TipsetKey, header::RawBlockHeader,
    };
    use crate::shim::address::Address;
    use crate::utils::multihash::prelude::*;
    use cid::Cid;
    use fvm_ipld_encoding::DAG_CBOR;
    use num_bigint::BigInt;
    use std::iter;

    pub fn mock_block(id: u64, weight: u64, ticket_sequence: u64) -> CachingBlockHeader {
        let addr = Address::new_id(id);
        let cid =
            Cid::try_from("bafyreicmaj5hhoy5mgqvamfhgexxyergw7hdeshizghodwkjg6qmpoco7i").unwrap();

        let fmt_str = format!("===={ticket_sequence}=====");
        let ticket = Ticket::new(VRFProof::new(fmt_str.clone().into_bytes()));
        let election_proof = ElectionProof {
            win_count: 0,
            vrfproof: VRFProof::new(fmt_str.into_bytes()),
        };
        let weight_inc = BigInt::from(weight);
        CachingBlockHeader::new(RawBlockHeader {
            miner_address: addr,
            election_proof: Some(election_proof),
            ticket: Some(ticket),
            message_receipts: cid,
            messages: cid,
            state_root: cid,
            weight: weight_inc,
            ..Default::default()
        })
    }

    #[test]
    fn test_break_weight_tie() {
        let b1 = mock_block(1234561, 1, 1);
        let ts1 = Tipset::from(&b1);

        let b2 = mock_block(1234562, 1, 2);
        let ts2 = Tipset::from(&b2);

        let b3 = mock_block(1234563, 1, 1);
        let ts3 = Tipset::from(&b3);

        // All tipsets have the same weight (but it's not really important here)

        // Can break weight tie
        assert!(ts1.break_weight_tie(&ts2));
        // Can not break weight tie (because of same min tickets)
        assert!(!ts1.break_weight_tie(&ts3));

        // Values are chosen so that Ticket(b4) < Ticket(b5) < Ticket(b1)
        let b4 = mock_block(1234564, 1, 41);
        let b5 = mock_block(1234565, 1, 45);
        let ts4 = Tipset::new(vec![b4.clone(), b5.clone(), b1.clone()]).unwrap();
        let ts5 = Tipset::new(vec![b4.clone(), b5.clone(), b2]).unwrap();
        // Can break weight tie with several min tickets the same
        assert!(ts4.break_weight_tie(&ts5));

        let ts6 = Tipset::new(vec![b4.clone(), b5.clone(), b1.clone()]).unwrap();
        let ts7 = Tipset::new(vec![b4, b5, b1]).unwrap();
        // Can not break weight tie with all min tickets the same
        assert!(!ts6.break_weight_tie(&ts7));
    }

    #[test]
    fn ensure_miner_addresses_are_distinct() {
        let h0 = RawBlockHeader {
            miner_address: Address::new_id(0),
            ..Default::default()
        };
        let h1 = RawBlockHeader {
            miner_address: Address::new_id(0),
            ..Default::default()
        };
        assert_eq!(
            Tipset::new([h0.clone(), h1.clone()]).unwrap_err(),
            CreateTipsetError::DuplicateMiner
        );

        let h_unique = RawBlockHeader {
            miner_address: Address::new_id(1),
            ..Default::default()
        };

        assert_eq!(
            Tipset::new([h_unique, h0, h1]).unwrap_err(),
            CreateTipsetError::DuplicateMiner
        );
    }

    #[test]
    fn ensure_epochs_are_equal() {
        let h0 = RawBlockHeader {
            miner_address: Address::new_id(0),
            epoch: 1,
            ..Default::default()
        };
        let h1 = RawBlockHeader {
            miner_address: Address::new_id(1),
            epoch: 2,
            ..Default::default()
        };
        assert_eq!(
            Tipset::new([h0, h1]).unwrap_err(),
            CreateTipsetError::BadEpoch
        );
    }

    #[test]
    fn ensure_state_roots_are_equal() {
        let h0 = RawBlockHeader {
            miner_address: Address::new_id(0),
            state_root: Cid::new_v1(DAG_CBOR, MultihashCode::Identity.digest(&[])),
            ..Default::default()
        };
        let h1 = RawBlockHeader {
            miner_address: Address::new_id(1),
            state_root: Cid::new_v1(DAG_CBOR, MultihashCode::Identity.digest(&[1])),
            ..Default::default()
        };
        assert_eq!(
            Tipset::new([h0, h1]).unwrap_err(),
            CreateTipsetError::BadStateRoot
        );
    }

    #[test]
    fn ensure_parent_cids_are_equal() {
        let h0 = RawBlockHeader {
            miner_address: Address::new_id(0),
            ..Default::default()
        };
        let h1 = RawBlockHeader {
            miner_address: Address::new_id(1),
            parents: TipsetKey::from(nonempty![Cid::new_v1(
                DAG_CBOR,
                MultihashCode::Identity.digest(&[])
            )]),
            ..Default::default()
        };
        assert_eq!(
            Tipset::new([h0, h1]).unwrap_err(),
            CreateTipsetError::BadParents
        );
    }

    #[test]
    fn ensure_there_are_blocks() {
        assert_eq!(
            Tipset::new(iter::empty::<RawBlockHeader>()).unwrap_err(),
            CreateTipsetError::Empty
        );
    }
}
