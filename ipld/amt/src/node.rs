// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

use crate::{nodes_for_height, BlockStore, Error, WIDTH};
use cid::Cid;
use encoding::{
    de::{self, Deserialize},
    ser,
    serde_bytes::{ByteBuf, Bytes},
};
use std::u8;

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

/// function puts values from vector into shard array
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

// ? Can maybe combined with vec_to_values later
/// Convert cids into linknode array
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

        // Get bitmap byte from serialized bytes
        let bmap: u8 = bmap_bz
            .get(0)
            .cloned()
            .ok_or_else(|| de::Error::custom("Expected bitmap byte"))?;

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
        Self { bmap, vals }
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
        height: u32,
        i: u64,
    ) -> Result<Option<Vec<u8>>, Error> {
        let sub_i = i / nodes_for_height(height);
        if !self.get_bit(sub_i) {
            return Ok(None);
        }
        if height == 0 {
            if let Values::Leaf(v) = &self.vals {
                return Ok(Some(v[i as usize].clone()));
            }

            return Ok(None);
        }
        todo!()
    }

    /// set value in node
    pub(super) fn set<DB: BlockStore>(
        &mut self,
        _bs: &DB,
        height: u32,
        i: u64,
        val: &[u8],
    ) -> Result<bool, Error> {
        if height == 0 {
            return Ok(self.set_leaf(i, val));
        }
        todo!()
    }

    fn set_leaf(&mut self, i: u64, val: &[u8]) -> bool {
        let already_set = self.get_bit(i);

        match &mut self.vals {
            Values::Leaf(v) => {
                v[i as usize] = val.to_vec();
                self.set_bit(i);
                already_set
            }
            Values::Links(_) => panic!("set_leaf should never be called on a shard of links"),
        }
    }

    /// Get bit from bitmap by index
    fn get_bit(&self, i: u64) -> bool {
        self.bmap & (1 << i) != 0
    }

    /// Set bit in bitmap for index
    fn set_bit(&mut self, i: u64) {
        self.bmap |= 1 << i;
    }

    /// Clear bit at index for bitmap
    #[allow(dead_code)] // TODO remove
    fn clear_bit(&mut self, i: u64) {
        self.bmap &= u8::MAX - (1 << i)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bitmap() {
        let mut node = Node {
            bmap: 0,
            vals: Values::Leaf(Default::default()),
        };
        assert_eq!(node.bmap, 0);
        node.set_bit(1);
        assert_eq!(node.get_bit(1), true);
        assert_eq!(node.bmap, 0b10);
        node.clear_bit(1);
        node.set_bit(0);
        assert_eq!(node.get_bit(0), true);
        assert_eq!(node.bmap, 0b1);
        node.set_bit(7);
        assert_eq!(node.get_bit(7), true);
        assert_eq!(node.bmap, 0b10000001);
    }
}
