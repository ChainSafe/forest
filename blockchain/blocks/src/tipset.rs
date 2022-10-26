// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{Block, BlockHeader, Error, Ticket};
use cid::Cid;
use fvm_ipld_encoding::Cbor;
use fvm_shared::clock::ChainEpoch;
use fvm_shared::{address::Address, bigint::BigInt};
use log::info;
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// A set of `CIDs` forming a unique key for a Tipset.
/// Equal keys will have equivalent iteration order, but note that the `CIDs` are *not* maintained in
/// the same order as the canonical iteration order of blocks in a tipset (which is by ticket)
#[derive(Clone, Debug, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TipsetKeys {
    pub cids: Vec<Cid>,
}

impl TipsetKeys {
    pub fn new(cids: Vec<Cid>) -> Self {
        Self { cids }
    }

    /// Returns tipset header `cids`
    pub fn cids(&self) -> &[Cid] {
        &self.cids
    }
}

impl Cbor for TipsetKeys {}

/// An immutable set of blocks at the same height with the same parent set.
/// Blocks in a tipset are canonically ordered by ticket size.
#[derive(Clone, Debug)]
pub struct Tipset {
    headers: Vec<BlockHeader>,
    key: OnceCell<TipsetKeys>,
}

impl PartialEq for Tipset {
    fn eq(&self, other: &Self) -> bool {
        self.headers.eq(&other.headers)
    }
}

impl quickcheck::Arbitrary for Tipset {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        // XXX: Support random generation of tipsets with multiple blocks.
        let first_header = BlockHeader::arbitrary(g);
        Tipset::new(vec![first_header]).unwrap()
    }
}

#[cfg(test)]
mod property_tests {
    use crate::ArbitraryCid;

    use super::tipset_json::{TipsetJson, TipsetJsonRef};
    use super::tipset_keys_json::TipsetKeysJson;
    use super::{Tipset, TipsetKeys};
    use cid::Cid;
    use quickcheck_macros::quickcheck;
    use serde_json;

    impl quickcheck::Arbitrary for TipsetKeys {
        fn arbitrary(g: &mut quickcheck::Gen) -> Self {
            let arbitrary_cids: Vec<ArbitraryCid> = Vec::arbitrary(g);
            let cids: Vec<Cid> = arbitrary_cids.iter().map(|cid| cid.0).collect();
            Self { cids }
        }
    }

    #[quickcheck]
    fn tipset_keys_roundtrip(tipset_keys: TipsetKeys) {
        let serialized = serde_json::to_string(&TipsetKeysJson(tipset_keys.clone())).unwrap();
        let parsed: TipsetKeysJson = serde_json::from_str(&serialized).unwrap();
        assert_eq!(tipset_keys, parsed.0);
    }

    #[quickcheck]
    fn tipset_roundtrip(tipset: Tipset) {
        let serialized = serde_json::to_string(&TipsetJsonRef(&tipset)).unwrap();
        let parsed: TipsetJson = serde_json::from_str(&serialized).unwrap();
        assert_eq!(&tipset, parsed.0.as_ref());
    }
}

impl From<FullTipset> for Tipset {
    fn from(full_tipset: FullTipset) -> Self {
        let key = full_tipset.key;
        let headers: Vec<BlockHeader> = full_tipset
            .blocks
            .into_iter()
            .map(|block| block.header)
            .collect();

        Tipset { headers, key }
    }
}

#[allow(clippy::len_without_is_empty)]
impl Tipset {
    /// Builds a new Tipset from a collection of blocks.
    /// A valid tipset contains a non-empty collection of blocks that have distinct miners and all
    /// specify identical epoch, parents, weight, height, state root, receipt root;
    /// content-id for headers are supposed to be distinct but until encoding is added will be equal.
    pub fn new(mut headers: Vec<BlockHeader>) -> Result<Self, Error> {
        verify_blocks(&headers)?;

        // sort headers by ticket size
        // break ticket ties with the header CIDs, which are distinct
        headers.sort_by_cached_key(|h| h.to_sort_key());

        // return tipset where sorted headers have smallest ticket size in the 0th index
        // and the distinct keys
        Ok(Self {
            headers,
            key: OnceCell::new(),
        })
    }
    /// Returns epoch of the tipset.
    pub fn epoch(&self) -> ChainEpoch {
        self.min_ticket_block().epoch()
    }
    /// Returns all blocks in tipset.
    pub fn blocks(&self) -> &[BlockHeader] {
        &self.headers
    }
    /// Consumes tipset to convert into a vector of [`BlockHeader`].
    pub fn into_blocks(self) -> Vec<BlockHeader> {
        self.headers
    }
    /// Returns the smallest ticket of all blocks in the tipset
    pub fn min_ticket(&self) -> Option<&Ticket> {
        self.min_ticket_block().ticket().as_ref()
    }
    /// Returns the block with the smallest ticket of all blocks in the tipset
    pub fn min_ticket_block(&self) -> &BlockHeader {
        // `Tipset::new` guarantees that `blocks` isn't empty
        self.headers.first().unwrap()
    }
    /// Returns the smallest timestamp of all blocks in the tipset
    pub fn min_timestamp(&self) -> u64 {
        self.headers
            .iter()
            .map(|block| block.timestamp())
            .min()
            .unwrap()
    }
    /// Returns the number of blocks in the tipset.
    pub fn len(&self) -> usize {
        self.headers.len()
    }
    /// Returns a key for the tipset.
    pub fn key(&self) -> &TipsetKeys {
        self.key.get_or_init(|| {
            TipsetKeys::new(self.headers.iter().map(BlockHeader::cid).cloned().collect())
        })
    }
    /// Returns slice of `CIDs` for the current tipset
    pub fn cids(&self) -> &[Cid] {
        self.key().cids()
    }
    /// Returns the keys of the parents of the blocks in the tipset.
    pub fn parents(&self) -> &TipsetKeys {
        self.min_ticket_block().parents()
    }
    /// Returns the state root for the tipset parent.
    pub fn parent_state(&self) -> &Cid {
        self.min_ticket_block().state_root()
    }
    /// Returns the tipset's calculated weight
    pub fn weight(&self) -> &BigInt {
        self.min_ticket_block().weight()
    }
    /// Returns true if self wins according to the Filecoin tie-break rule (FIP-0023)
    pub fn break_weight_tie(&self, other: &Tipset) -> bool {
        // blocks are already sorted by ticket
        let broken = self
            .blocks()
            .iter()
            .zip(other.blocks().iter())
            .any(|(a, b)| {
                const MSG: &str =
                    "The function block_sanity_checks should have been called at this point.";
                let ticket = a.ticket().as_ref().expect(MSG);
                let other_ticket = b.ticket().as_ref().expect(MSG);
                ticket.vrfproof < other_ticket.vrfproof
            });
        if broken {
            info!("weight tie broken in favour of {:?}", self.key());
        } else {
            info!("weight tie left unbroken, default to {:?}", other.key());
        }
        broken
    }
}

/// `FullTipset` is an expanded version of a tipset that contains all the blocks and messages
#[derive(Debug, Clone)]
pub struct FullTipset {
    blocks: Vec<Block>,
    key: OnceCell<TipsetKeys>,
}

impl PartialEq for FullTipset {
    fn eq(&self, other: &Self) -> bool {
        self.blocks.eq(&other.blocks)
    }
}

impl FullTipset {
    pub fn new(mut blocks: Vec<Block>) -> Result<Self, Error> {
        verify_blocks(blocks.iter().map(Block::header))?;

        // sort blocks on creation to allow for more seamless conversions between FullTipset
        // and Tipset
        blocks.sort_by_cached_key(|block| block.header().to_sort_key());
        Ok(Self {
            blocks,
            key: OnceCell::new(),
        })
    }
    /// Returns the first block of the tipset.
    fn first_block(&self) -> &Block {
        // `FullTipset::new` guarantees that `blocks` isn't empty
        self.blocks.first().unwrap()
    }
    /// Returns reference to all blocks in a full tipset.
    pub fn blocks(&self) -> &[Block] {
        &self.blocks
    }
    /// Returns all blocks in a full tipset.
    pub fn into_blocks(self) -> Vec<Block> {
        self.blocks
    }
    /// Converts the full tipset into a [Tipset] which removes the messages attached.
    pub fn into_tipset(self) -> Tipset {
        Tipset::from(self)
    }
    /// Returns a key for the tipset.
    pub fn key(&self) -> &TipsetKeys {
        self.key
            .get_or_init(|| TipsetKeys::new(self.blocks.iter().map(Block::cid).cloned().collect()))
    }
    /// Returns the state root for the tipset parent.
    pub fn parent_state(&self) -> &Cid {
        self.first_block().header().state_root()
    }
    /// Returns epoch of the tipset.
    pub fn epoch(&self) -> ChainEpoch {
        self.first_block().header().epoch()
    }
    /// Returns the tipset's calculated weight.
    pub fn weight(&self) -> &BigInt {
        self.first_block().header().weight()
    }
}

fn verify_blocks<'a, I>(headers: I) -> Result<(), Error>
where
    I: IntoIterator<Item = &'a BlockHeader>,
{
    let mut headers = headers.into_iter();
    let first_header = headers.next().ok_or(Error::NoBlocks)?;

    let verify = |predicate: bool, message: &'static str| {
        if predicate {
            Ok(())
        } else {
            Err(Error::InvalidTipset(message.to_string()))
        }
    };

    let mut headers_set: HashSet<Address> = HashSet::new();
    headers_set.insert(*first_header.miner_address());

    for header in headers {
        verify(
            header.parents() == first_header.parents(),
            "parent cids are not equal",
        )?;
        verify(
            header.state_root() == first_header.state_root(),
            "state_roots are not equal",
        )?;
        verify(
            header.epoch() == first_header.epoch(),
            "epochs are not equal",
        )?;

        verify(
            headers_set.insert(*header.miner_address()),
            "miner_addresses are not distinct",
        )?;
    }

    Ok(())
}

pub mod tipset_keys_json {
    use super::*;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    #[derive(Clone, Debug, Deserialize, Serialize)]
    #[serde(transparent)]
    pub struct TipsetKeysJson(#[serde(with = "self")] pub TipsetKeys);

    impl From<TipsetKeysJson> for TipsetKeys {
        fn from(wrapper: TipsetKeysJson) -> Self {
            wrapper.0
        }
    }

    impl From<TipsetKeys> for TipsetKeysJson {
        fn from(wrapper: TipsetKeys) -> Self {
            TipsetKeysJson(wrapper)
        }
    }

    pub fn serialize<S>(m: &TipsetKeys, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        forest_json::cid::vec::serialize(m.cids(), serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<TipsetKeys, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(TipsetKeys {
            cids: forest_json::cid::vec::deserialize(deserializer)?,
        })
    }
}

pub mod tipset_json {
    use super::*;
    use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
    use std::sync::Arc;

    /// Wrapper for serializing and de-serializing a `Tipset` from JSON.
    #[derive(Debug, Deserialize, Serialize)]
    #[serde(transparent)]
    pub struct TipsetJson(#[serde(with = "self")] pub Arc<Tipset>);

    /// Wrapper for serializing a `Tipset` reference to JSON.
    #[derive(Serialize)]
    #[serde(transparent)]
    pub struct TipsetJsonRef<'a>(#[serde(with = "self")] pub &'a Tipset);

    impl From<TipsetJson> for Arc<Tipset> {
        fn from(wrapper: TipsetJson) -> Self {
            wrapper.0
        }
    }

    impl From<Arc<Tipset>> for TipsetJson {
        fn from(wrapper: Arc<Tipset>) -> Self {
            TipsetJson(wrapper)
        }
    }

    impl<'a> From<&'a Tipset> for TipsetJsonRef<'a> {
        fn from(wrapper: &'a Tipset) -> Self {
            TipsetJsonRef(wrapper)
        }
    }

    pub fn serialize<S>(m: &Tipset, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        #[derive(Serialize)]
        #[serde(rename_all = "PascalCase")]
        struct TipsetSer<'a> {
            #[serde(with = "super::tipset_keys_json")]
            cids: &'a TipsetKeys,
            #[serde(with = "super::super::header::json::vec")]
            blocks: &'a [BlockHeader],
            height: ChainEpoch,
        }
        TipsetSer {
            blocks: &m.headers,
            cids: m.key(),
            height: m.epoch(),
        }
        .serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Arc<Tipset>, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Serialize, Deserialize)]
        #[serde(rename_all = "PascalCase")]
        struct TipsetDe {
            #[serde(with = "super::tipset_keys_json")]
            cids: TipsetKeys,
            #[serde(with = "super::super::header::json::vec")]
            blocks: Vec<BlockHeader>,
            height: ChainEpoch,
        }
        let TipsetDe { blocks, .. } = Deserialize::deserialize(deserializer)?;
        Tipset::new(blocks).map(Arc::new).map_err(de::Error::custom)
    }
}

#[cfg(test)]
mod test {
    use crate::{BlockHeader, ElectionProof, Error, Ticket, Tipset, TipsetKeys};
    use cid::multihash::Code::Identity;
    use cid::multihash::MultihashDigest;
    use cid::Cid;
    use forest_crypto::VRFProof;
    use fvm_ipld_encoding::DAG_CBOR;
    use fvm_shared::address::Address;
    use num_bigint::BigInt;

    pub fn mock_block(id: u64, weight: u64, ticket_sequence: u64) -> BlockHeader {
        let addr = Address::new_id(id);
        let cid =
            Cid::try_from("bafyreicmaj5hhoy5mgqvamfhgexxyergw7hdeshizghodwkjg6qmpoco7i").unwrap();

        let fmt_str = format!("===={}=====", ticket_sequence);
        let ticket = Ticket::new(VRFProof::new(fmt_str.clone().into_bytes()));
        let election_proof = ElectionProof {
            win_count: 0,
            vrfproof: VRFProof::new(fmt_str.into_bytes()),
        };
        let weight_inc = BigInt::from(weight);
        BlockHeader::builder()
            .miner_address(addr)
            .election_proof(Some(election_proof))
            .ticket(Some(ticket))
            .message_receipts(cid)
            .messages(cid)
            .state_root(cid)
            .weight(weight_inc)
            .build()
            .unwrap()
    }

    #[test]
    fn test_break_weight_tie() {
        let b1 = mock_block(1234561, 1, 1);
        let ts1 = Tipset::new(vec![b1.clone()]).unwrap();

        let b2 = mock_block(1234562, 1, 2);
        let ts2 = Tipset::new(vec![b2.clone()]).unwrap();

        let b3 = mock_block(1234563, 1, 1);
        let ts3 = Tipset::new(vec![b3]).unwrap();

        // All tipsets have the same weight (but it's not really important here)

        // Can break weight tie
        assert!(ts1.break_weight_tie(&ts2));
        // Can not break weight tie (because of same min tickets)
        assert!(!ts1.break_weight_tie(&ts3));

        // Values are choosen so that Ticket(b4) < Ticket(b5) < Ticket(b1)
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
        let h0 = BlockHeader::builder()
            .miner_address(Address::new_id(0))
            .build()
            .unwrap();
        let h1 = BlockHeader::builder()
            .miner_address(Address::new_id(0))
            .build()
            .unwrap();
        assert_eq!(
            Tipset::new(vec![h0, h1]).unwrap_err(),
            Error::InvalidTipset("miner_addresses are not distinct".to_string())
        );
    }

    // specifically test the case when we are distinct from miner_address 0, but not 1
    #[test]
    fn ensure_multiple_miner_addresses_are_distinct() {
        let h0 = BlockHeader::builder()
            .miner_address(Address::new_id(1))
            .build()
            .unwrap();
        let h1 = BlockHeader::builder()
            .miner_address(Address::new_id(0))
            .build()
            .unwrap();
        let h2 = BlockHeader::builder()
            .miner_address(Address::new_id(0))
            .build()
            .unwrap();
        assert_eq!(
            Tipset::new(vec![h0, h1, h2]).unwrap_err(),
            Error::InvalidTipset("miner_addresses are not distinct".to_string())
        );
    }

    #[test]
    fn ensure_epochs_are_equal() {
        let h0 = BlockHeader::builder()
            .miner_address(Address::new_id(0))
            .epoch(1)
            .build()
            .unwrap();
        let h1 = BlockHeader::builder()
            .miner_address(Address::new_id(1))
            .epoch(2)
            .build()
            .unwrap();
        assert_eq!(
            Tipset::new(vec![h0, h1]).unwrap_err(),
            Error::InvalidTipset("epochs are not equal".to_string())
        );
    }

    #[test]
    fn ensure_state_roots_are_equal() {
        let h0 = BlockHeader::builder()
            .miner_address(Address::new_id(0))
            .state_root(Cid::new_v1(DAG_CBOR, Identity.digest(&[])))
            .build()
            .unwrap();
        let h1 = BlockHeader::builder()
            .miner_address(Address::new_id(1))
            .state_root(Cid::new_v1(DAG_CBOR, Identity.digest(&[1])))
            .build()
            .unwrap();
        assert_eq!(
            Tipset::new(vec![h0, h1]).unwrap_err(),
            Error::InvalidTipset("state_roots are not equal".to_string())
        );
    }

    #[test]
    fn ensure_parent_cids_are_equal() {
        let h0 = BlockHeader::builder()
            .miner_address(Address::new_id(0))
            .parents(TipsetKeys::default())
            .build()
            .unwrap();
        let h1 = BlockHeader::builder()
            .miner_address(Address::new_id(1))
            .parents(TipsetKeys::new(vec![Cid::new_v1(
                DAG_CBOR,
                Identity.digest(&[]),
            )]))
            .build()
            .unwrap();
        assert_eq!(
            Tipset::new(vec![h0, h1]).unwrap_err(),
            Error::InvalidTipset("parent cids are not equal".to_string())
        );
    }

    #[test]
    fn ensure_there_are_blocks() {
        assert_eq!(Tipset::new(vec![]).unwrap_err(), Error::NoBlocks);
    }
}
