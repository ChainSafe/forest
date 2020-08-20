// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{Block, BlockHeader, Error, Ticket};
use cid::Cid;
use clock::ChainEpoch;
use encoding::Cbor;
use num_bigint::BigInt;
use serde::{Deserialize, Serialize};

/// A set of CIDs forming a unique key for a Tipset.
/// Equal keys will have equivalent iteration order, but note that the CIDs are *not* maintained in
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

    /// Returns tipset header cids
    pub fn cids(&self) -> &[Cid] {
        &self.cids
    }
}

impl Cbor for TipsetKeys {}

/// An immutable set of blocks at the same height with the same parent set.
/// Blocks in a tipset are canonically ordered by ticket size.
#[derive(Clone, PartialEq, Debug, Eq)]
pub struct Tipset {
    blocks: Vec<BlockHeader>,
    key: TipsetKeys,
}

impl From<FullTipset> for Tipset {
    fn from(full_tipset: FullTipset) -> Self {
        let block_headers: Vec<BlockHeader> = full_tipset
            .blocks
            .into_iter()
            .map(|block| block.header)
            .collect();
        let cids = block_headers
            .iter()
            .map(BlockHeader::cid)
            .cloned()
            .collect();

        Tipset {
            blocks: block_headers,
            key: TipsetKeys { cids },
        }
    }
}

#[allow(clippy::len_without_is_empty)]
impl Tipset {
    /// Builds a new Tipset from a collection of blocks.
    /// A valid tipset contains a non-empty collection of blocks that have distinct miners and all
    /// specify identical epoch, parents, weight, height, state root, receipt root;
    /// contentID for headers are supposed to be distinct but until encoding is added will be equal.
    pub fn new(mut headers: Vec<BlockHeader>) -> Result<Self, Error> {
        verify_blocks(&headers)?;

        // TODO Have a check the ensures CIDs are distinct
        let cids = headers.iter().map(BlockHeader::cid).cloned().collect();

        // sort headers by ticket size
        // break ticket ties with the header CIDs, which are distinct
        headers.sort();

        // return tipset where sorted headers have smallest ticket size in the 0th index
        // and the distinct keys
        Ok(Self {
            blocks: headers,
            key: TipsetKeys {
                // interim until CID check is in place
                cids,
            },
        })
    }
    /// Returns epoch of the tipset
    pub fn epoch(&self) -> ChainEpoch {
        self.min_ticket_block().epoch()
    }
    /// Returns all blocks in tipset
    pub fn blocks(&self) -> &[BlockHeader] {
        &self.blocks
    }
    /// Returns all blocks in tipset
    pub fn into_blocks(self) -> Vec<BlockHeader> {
        self.blocks
    }
    /// Returns the smallest ticket of all blocks in the tipset
    pub fn min_ticket(&self) -> Ticket {
        self.min_ticket_block().ticket().clone()
    }
    /// Returns the block with the smallest ticket of all blocks in the tipset
    pub fn min_ticket_block(&self) -> &BlockHeader {
        // `Tipset::new` guarantees that `blocks` isn't empty
        self.blocks.first().unwrap()
    }
    /// Returns the smallest timestamp of all blocks in the tipset
    pub fn min_timestamp(&self) -> u64 {
        self.blocks
            .iter()
            .map(|block| block.timestamp())
            .min()
            .unwrap()
    }
    /// Returns the number of blocks in the tipset
    pub fn len(&self) -> usize {
        self.blocks.len()
    }
    /// Returns a key for the tipset.
    pub fn key(&self) -> &TipsetKeys {
        &self.key
    }
    /// Returns slice of Cids for the current tipset
    pub fn cids(&self) -> &[Cid] {
        self.key.cids()
    }
    /// Returns the CIDs of the parents of the blocks in the tipset
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
}

/// FullTipset is an expanded version of the Tipset that contains all the blocks and messages
#[derive(Debug, PartialEq, Clone)]
pub struct FullTipset {
    blocks: Vec<Block>,
}

impl FullTipset {
    /// constructor
    pub fn new(mut blocks: Vec<Block>) -> Result<Self, Error> {
        verify_blocks(blocks.iter().map(Block::header))?;

        // sort blocks on creation to allow for more seamless conversions between FullTipset
        // and Tipset
        blocks.sort_by(|block1, block2| block1.header().cmp(block2.header()));
        Ok(Self { blocks })
    }
    /// Returns the first block of the tipset
    fn first_block(&self) -> &Block {
        // `FullTipset::new` guarantees that `blocks` isn't empty
        self.blocks.first().unwrap()
    }
    /// Returns reference to all blocks in a full tipset
    pub fn blocks(&self) -> &[Block] {
        &self.blocks
    }
    /// Returns all blocks in a full tipset
    pub fn into_blocks(self) -> Vec<Block> {
        self.blocks
    }
    /// Returns a Tipset
    pub fn to_tipset(&self) -> Tipset {
        let block_headers: Vec<BlockHeader> =
            self.blocks.iter().map(Block::header).cloned().collect();
        let cids = block_headers
            .iter()
            .map(BlockHeader::cid)
            .cloned()
            .collect();

        Tipset {
            blocks: block_headers,
            key: TipsetKeys { cids },
        }
    }
    /// Returns the state root for the tipset parent.
    pub fn parent_state(&self) -> &Cid {
        self.first_block().header().state_root()
    }
    /// Returns epoch of the tipset
    pub fn epoch(&self) -> ChainEpoch {
        self.first_block().header().epoch()
    }
    /// Returns the tipset's calculated weight
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

    for header in headers {
        verify(
            header.parents() == first_header.parents(),
            "parent cids are not equal",
        )?;
        verify(
            header.weight() == first_header.weight(),
            "weights are not equal",
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
            header.message_receipts() == first_header.message_receipts(),
            "message_receipts are not equal",
        )?;
        verify(
            header.miner_address() != first_header.miner_address(),
            "miner_addresses are not distinct",
        )?;
    }

    Ok(())
}

#[cfg(feature = "json")]
pub mod tipset_keys_json {
    use super::*;
    use serde::{Deserializer, Serializer};

    pub fn serialize<S>(m: &TipsetKeys, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        cid::json::vec::serialize(m.cids(), serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<TipsetKeys, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(TipsetKeys {
            cids: cid::json::vec::deserialize(deserializer)?,
        })
    }
}

#[cfg(feature = "json")]
pub mod tipset_json {
    use super::*;
    use serde::{de, Deserialize, Deserializer, Serialize, Serializer};

    /// Wrapper for serializing and deserializing a SignedMessage from JSON.
    #[derive(Deserialize, Serialize, Debug)]
    #[serde(transparent)]
    pub struct TipsetJson(#[serde(with = "self")] pub Tipset);

    /// Wrapper for serializing a SignedMessage reference to JSON.
    #[derive(Serialize)]
    #[serde(transparent)]
    pub struct TipsetJsonRef<'a>(#[serde(with = "self")] pub &'a Tipset);

    impl From<TipsetJson> for Tipset {
        fn from(wrapper: TipsetJson) -> Self {
            wrapper.0
        }
    }

    impl From<Tipset> for TipsetJson {
        fn from(wrapper: Tipset) -> Self {
            TipsetJson(wrapper)
        }
    }

    pub fn serialize<S>(m: &Tipset, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        #[derive(Serialize)]
        #[serde(rename_all = "PascalCase")]
        struct TipsetSer<'a> {
            #[serde(with = "super::super::header::json::vec")]
            blocks: &'a [BlockHeader],
            #[serde(with = "super::tipset_keys_json")]
            cids: &'a TipsetKeys,
            height: ChainEpoch,
        }
        TipsetSer {
            blocks: &m.blocks,
            cids: &m.key,
            height: m.epoch(),
        }
        .serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Tipset, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Serialize, Deserialize)]
        #[serde(rename_all = "PascalCase")]
        struct TipsetDe {
            #[serde(with = "super::super::header::json::vec")]
            blocks: Vec<BlockHeader>,
            #[serde(with = "super::tipset_keys_json")]
            cids: TipsetKeys,
            height: ChainEpoch,
        }
        let TipsetDe { blocks, .. } = Deserialize::deserialize(deserializer)?;
        Tipset::new(blocks).map_err(de::Error::custom)
    }
}
