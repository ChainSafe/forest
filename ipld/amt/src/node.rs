// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

use crate::{nodes_for_height, BitMap, BlockStore, Error, WIDTH};
use cid::Cid;
use encoding::{
    de::{self, Deserialize},
    from_slice, ser,
    serde_bytes::{ByteBuf, Bytes},
};
use std::u8;

/// This represents a link to another Node
#[derive(PartialEq, Eq, Clone, Debug)]
pub(super) enum LinkNode {
    Cid(Cid),
    Empty,
    Cached(Box<Node>),
}

impl Default for LinkNode {
    fn default() -> Self {
        LinkNode::Empty
    }
}

impl From<Cid> for LinkNode {
    fn from(c: Cid) -> LinkNode {
        LinkNode::Cid(c)
    }
}

/// Values represents the underlying data of a node, whether it is a link or leaf node
#[derive(PartialEq, Eq, Clone, Debug)]
pub(super) enum Values {
    Links([LinkNode; WIDTH]),
    Leaf([Vec<u8>; WIDTH]),
}

impl Default for Values {
    fn default() -> Self {
        Values::Leaf(Default::default())
    }
}

/// Node represents either a shard of values in the form of bytes or links to other nodes
#[derive(PartialEq, Eq, Clone, Debug, Default)]
pub(super) struct Node {
    pub(super) bmap: BitMap,
    pub(super) vals: Values,
}

/// Turns the WIDTH length array into a vector for serialization
fn values_to_vec<T: Clone>(bmap: BitMap, values: &[T; WIDTH]) -> Vec<T> {
    let mut v: Vec<T> = Vec::new();
    for (i, _) in values.iter().enumerate().take(WIDTH) {
        if bmap.get_bit(i as u64) {
            v.push(values[i].clone())
        }
    }
    v
}

/// Puts values from vector into shard array
fn vec_to_values<V, T>(bmap: BitMap, values: Vec<V>) -> Result<[T; WIDTH], Error>
where
    V: Clone,
    T: Default + From<V>,
{
    let mut r_arr: [T; WIDTH] = Default::default();

    let mut v_iter = values.iter();

    for (i, e) in r_arr.iter_mut().enumerate().take(WIDTH) {
        if bmap.get_bit(i as u64) {
            let value = v_iter
                .next()
                .ok_or_else(|| Error::Custom("Vector length does not match bitmap".to_owned()))?;
            *e = <T>::from(value.clone());
        }
    }

    Ok(r_arr)
}

/// Convert Link node into vector of Cids
fn cids_from_links(links: &[LinkNode; WIDTH]) -> Result<Vec<Cid>, Error> {
    links
        .iter()
        .filter_map(|c| match c {
            LinkNode::Cid(cid) => Some(Ok(cid.clone())),
            LinkNode::Cached(_) => Some(Err(Error::Cached)),
            LinkNode::Empty => None,
        })
        .collect()
}

impl ser::Serialize for Node {
    fn serialize<S>(&self, s: S) -> Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        let bmap_arr = self.bmap.to_byte_array();
        let bitmap_bz = Bytes::new(&bmap_arr);
        match &self.vals {
            Values::Leaf(v) => (bitmap_bz, [0u8; 0], values_to_vec(self.bmap, &v)).serialize(s),
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

        let values: Vec<Vec<u8>> = values.iter().map(|v| v.clone().into_vec()).collect();

        // Get bitmap byte from serialized bytes
        let bmap: BitMap = bmap_bz
            .get(0)
            .map(|b| BitMap::new(*b))
            .ok_or_else(|| de::Error::custom("Expected bitmap byte"))?;

        if links.is_empty() {
            let leaf_arr: [Vec<u8>; WIDTH] =
                vec_to_values(bmap, values).map_err(|e| de::Error::custom(e.to_string()))?;
            Ok(Self {
                bmap,
                vals: Values::Leaf(leaf_arr),
            })
        } else {
            let link_arr: [LinkNode; WIDTH] =
                vec_to_values(bmap, links).map_err(|e| de::Error::custom(e.to_string()))?;
            Ok(Self {
                bmap,
                vals: Values::Links(link_arr),
            })
        }
    }
}

impl Node {
    /// Constructor for node
    pub(super) fn new(bmap: u8, vals: Values) -> Self {
        Self {
            bmap: BitMap::new(bmap),
            vals,
        }
    }

    /// Flushes cache for node, replacing any cached values with a Cid variant
    pub(super) fn flush<DB: BlockStore>(&mut self, bs: &DB) -> Result<(), Error> {
        if let Values::Links(l) = &mut self.vals {
            for link in &mut l.iter_mut() {
                if let LinkNode::Cached(n) = link {
                    // flush sub node to clear caches
                    n.flush(bs)?;

                    // Puts node in blockstore and and retrieves it's CID
                    let cid = bs.put(n)?;

                    // Turn cached node into a Cid link
                    *link = LinkNode::Cid(cid);
                }
            }
        }

        Ok(())
    }

    /// Check if node is empty
    pub(super) fn empty(&self) -> bool {
        self.bmap.is_empty()
    }

    /// Gets value at given index of AMT given height
    pub(super) fn get<DB: BlockStore>(
        &mut self,
        bs: &DB,
        height: u32,
        i: u64,
    ) -> Result<Option<Vec<u8>>, Error> {
        let sub_i = i / nodes_for_height(height);
        if !self.bmap.get_bit(sub_i) {
            return Ok(None);
        }

        match &mut self.vals {
            Values::Leaf(v) => Ok(Some(v[i as usize].clone())),
            Values::Links(l) => match &mut l[sub_i as usize] {
                LinkNode::Cid(cid) => {
                    let res: Vec<u8> = bs.get(cid)?.ok_or_else(|| {
                        Error::Cid("Cid did not match any in database".to_owned())
                    })?;

                    // pass back node to be queried
                    // TODO after benchmarking check if cache should be updated from get
                    let mut node: Node = from_slice(&res)?;

                    node.get(bs, height - 1, i % nodes_for_height(height))
                }
                LinkNode::Cached(n) => n.get(bs, height - 1, i % nodes_for_height(height)),
                LinkNode::Empty => Ok(None),
            },
        }
    }

    /// Set value in node
    pub(super) fn set<DB: BlockStore>(
        &mut self,
        bs: &DB,
        height: u32,
        i: u64,
        val: &[u8],
    ) -> Result<bool, Error> {
        if height == 0 {
            return Ok(self.set_leaf(i, val));
        }

        let nfh = nodes_for_height(height);

        // If dividing by nodes for height should give an index for link in node
        let idx: usize = (i / nfh) as usize;
        assert!(idx < 8);

        if let Node {
            vals: Values::Links(links),
            bmap,
        } = self
        {
            links[idx] = match &mut links[idx] {
                LinkNode::Cid(cid) => {
                    let res: Vec<u8> = bs.get(cid)?.ok_or_else(|| {
                        Error::Cid("Cid did not match any in database".to_owned())
                    })?;

                    LinkNode::Cached(Box::new(from_slice(&res)?))
                }
                LinkNode::Empty => {
                    let node = match height {
                        1 => Node::new(0, Values::Leaf(Default::default())),
                        _ => Node::new(0, Values::Links(Default::default())),
                    };
                    bmap.set_bit(idx as u64);
                    LinkNode::Cached(Box::new(node))
                }
                LinkNode::Cached(node) => return node.set(bs, height - 1, i % nfh, val),
            };

            if let LinkNode::Cached(n) = &mut links[idx] {
                n.set(bs, height - 1, i % nfh, val)
            } else {
                unreachable!("Value is set as cached")
            }
        } else {
            unreachable!("Non zero height in AMT is always Links type")
        }
    }

    fn set_leaf(&mut self, i: u64, val: &[u8]) -> bool {
        let already_set = self.bmap.get_bit(i);

        match &mut self.vals {
            Values::Leaf(v) => {
                v[i as usize] = val.to_vec();
                self.bmap.set_bit(i);
                !already_set
            }
            Values::Links(_) => panic!("set_leaf should never be called on a shard of links"),
        }
    }

    /// Delete value in AMT by index
    pub(super) fn delete<DB: BlockStore>(
        &mut self,
        bs: &DB,
        height: u32,
        i: u64,
    ) -> Result<bool, Error> {
        let sub_i = i / nodes_for_height(height);

        if !self.bmap.get_bit(sub_i) {
            // Value does not exist in AMT
            return Ok(false);
        }

        match self {
            Self {
                vals: Values::Leaf(_),
                bmap,
            } => {
                assert_eq!(
                    height, 0,
                    "Height must be 0 when clearing bit for leaf node"
                );

                // When deleting from node, should only need to clear bit from bitmap
                bmap.clear_bit(i);
                Ok(true)
            }
            Self {
                vals: Values::Links(l),
                bmap,
            } => {
                let mut sub_node: Node = match &l[sub_i as usize] {
                    LinkNode::Cached(n) => *n.clone(),
                    LinkNode::Cid(cid) => {
                        let res: Vec<u8> = bs.get(cid)?.ok_or_else(|| {
                            Error::Cid("Cid did not match any in database".to_owned())
                        })?;

                        from_slice(&res)?
                    }
                    LinkNode::Empty => unreachable!("Bitmap value for index is set"),
                };

                // Follow node to delete from subnode
                if !sub_node.delete(bs, height - 1, i % nodes_for_height(height))? {
                    // Index to be deleted was not found
                    return Ok(false);
                }

                // Value was deleted, move node to cache or clear bit if removing shard
                l[sub_i as usize] = if sub_node.bmap.is_empty() {
                    bmap.clear_bit(sub_i);
                    LinkNode::Empty
                } else {
                    LinkNode::Cached(Box::new(sub_node))
                };

                Ok(true)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use encoding::{from_slice, to_vec};

    #[test]
    fn serialize_node_symmetric() {
        let node = Node::default();
        let nbz = to_vec(&node).unwrap();
        assert_eq!(from_slice::<Node>(&nbz).unwrap(), node);
    }
}
