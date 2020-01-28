// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

use crate::{BlockStore, Error, WIDTH};
use cid::Cid;
use encoding::{
    de::{self, Deserialize},
    ser,
    serde_bytes::{ByteBuf, Bytes},
};

#[derive(PartialEq, Eq, Clone, Debug)]
pub enum LinkNode {
    Cid(Cid),
    Cached(Box<Node>),
}

// TODO remove if unneeded
impl From<Cid> for LinkNode {
    fn from(c: Cid) -> LinkNode {
        LinkNode::Cid(c)
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub enum Values {
    Links([LinkNode; WIDTH]),
    Leaf([Vec<u8>; WIDTH]),
}

impl Default for Values {
    fn default() -> Self {
        Values::Leaf(Default::default())
    }
}

#[derive(PartialEq, Eq, Clone, Debug, Default)]
pub struct Node {
    pub(super) bmap: u8,
    pub(super) vals: Values,
}

/// function turns the WIDTH length array into a vector for serialization
fn values_to_vec<T>(_bmap: u8, _values: [T; WIDTH]) -> Vec<T> {
    todo!()
}

/// function turns the WIDTH length array into a vector for serialization
fn vec_to_values<T>(_bmap: u8, _values: Vec<T>) -> [T; WIDTH] {
    todo!()
}

/// Convert Link node into
fn cids_from_links(links: &[LinkNode]) -> Result<Vec<Cid>, Error> {
    links
        .iter()
        .map(|c| match c {
            LinkNode::Cid(cid) => Ok(cid.clone()),
            LinkNode::Cached(_) => Err(Error::Cached),
        })
        .collect()
}

fn cids_to_arr(_bmap: u8, _values: Vec<Cid>) -> [LinkNode; WIDTH] {
    todo!()
}

impl ser::Serialize for Node {
    fn serialize<S>(&self, s: S) -> Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        let bmap_arr = [self.bmap];
        let bitmap_bz = Bytes::new(&bmap_arr);
        match &self.vals {
            // TODO confirm that 0 array of 0u8 will serialize correctly
            Values::Leaf(v) => {
                (bitmap_bz, [0u8; 0], values_to_vec(self.bmap, v.clone())).serialize(s)
            }
            Values::Links(v) => {
                let cids = cids_from_links(v).map_err(|e| ser::Error::custom(e.to_string()))?;
                (bitmap_bz, cids, [0u8; 0]).serialize(s)
            }
        }
    }
}

impl<'de> de::Deserialize<'de> for Node {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        let (bmap_bz, links, values): (ByteBuf, Vec<Cid>, Vec<ByteBuf>) =
            Deserialize::deserialize(deserializer)?;

        // TODO see if possible to remove bytebuf clone
        let values: Vec<Vec<u8>> = values.iter().map(|v| v.clone().into_vec()).collect();

        // TODO make sure it's safe to index like this (should be)
        let bmap: u8 = bmap_bz.as_slice()[0];
        if links.is_empty() {
            let leaf_arr: [Vec<u8>; WIDTH] = vec_to_values(bmap, values);
            Ok(Self {
                bmap,
                vals: Values::Leaf(leaf_arr),
            })
        } else {
            let link_arr: [LinkNode; WIDTH] = cids_to_arr(bmap, links);
            Ok(Self {
                bmap,
                vals: Values::Links(link_arr),
            })
        }
    }
}

impl Node {
    /// Constructor
    pub fn new(bmap: u8, vals: Values) -> Self {
        Self {
            bmap,
            vals,
        }
    }
    pub fn flush<DB: BlockStore>(&mut self, _bs: &DB, _depth: u32) -> Result<(), Error> {
        // TODO
        todo!()
    }
    /// Check if node is empty
    pub(super) fn empty(&self) -> bool {
        self.bmap == 0
    }
    /// Check if node is empty
    pub(super) fn get<DB: BlockStore>(
        &mut self,
        _bs: &DB,
        _height: u32,
        _i: u64,
    ) -> Result<Option<Vec<u8>>, Error> {
        todo!()
    }
    /// set value in node
    pub(super) fn set<DB: BlockStore>(
        &mut self,
        _bs: &DB,
        height: u32,
        _i: u64,
        _val: &[u8],
    ) -> Result<bool, Error> {
        if height == 0 {}
        todo!()
    }
    // pub(super) fn load_node<DB: BlockStore>(
    //     &mut self,
    //     _bs: &DB,
    //     i: u64,
    //     _create: bool,
    // ) -> Result<Node, Error> {
    //     // if self.cache.is_empty() {
    //     //     self.expand_links();
    //     // } else if let Some(v) = self.cache.get(i as usize) {
    //     //     return Ok(Node::clone(v));
    //     // }

    //     todo!()
    // }
}
