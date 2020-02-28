// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::{nodes_for_height, BitMap, Error, WIDTH};
use cid::Cid;
use encoding::{
    de::{self, Deserialize, DeserializeOwned},
    ser::{self, Serialize},
};
use ipld_blockstore::BlockStore;

/// This represents a link to another Node
#[derive(PartialEq, Eq, Clone, Debug)]
pub(super) enum Link<V> {
    Cid(Cid),
    Cached(Box<Node<V>>),
}

impl<V> From<Cid> for Link<V> {
    fn from(c: Cid) -> Link<V> {
        Link::Cid(c)
    }
}

/// Node represents either a shard of values in the form of bytes or links to other nodes
#[derive(PartialEq, Eq, Clone, Debug)]
// TODO benchmark boxing all variables
#[allow(clippy::large_enum_variant)]
pub(super) enum Node<V> {
    Link {
        bmap: BitMap,
        links: [Option<Link<V>>; WIDTH],
    },
    Leaf {
        bmap: BitMap,
        vals: [Option<V>; WIDTH],
    },
}

impl<V> Default for Node<V> {
    fn default() -> Self {
        Node::Leaf {
            bmap: Default::default(),
            vals: Default::default(),
        }
    }
}

/// Turns the WIDTH length array into a vector for serialization
fn values_to_vec<T>(bmap: BitMap, values: &[Option<T>; WIDTH]) -> Vec<T>
where
    T: Clone,
{
    let mut v: Vec<T> = Vec::new();
    for (i, _) in values.iter().enumerate().take(WIDTH) {
        if bmap.get_bit(i as u64) {
            v.push(values[i].clone().unwrap())
        }
    }
    v
}

/// Puts values from vector into shard array
fn vec_to_values<V, T>(bmap: BitMap, values: Vec<V>) -> Result<[Option<T>; WIDTH], Error>
where
    V: Clone,
    T: From<V>,
{
    let mut r_arr: [Option<T>; WIDTH] = Default::default();

    let mut v_iter = values.iter();

    for (i, e) in r_arr.iter_mut().enumerate().take(WIDTH) {
        if bmap.get_bit(i as u64) {
            let value = v_iter
                .next()
                .ok_or_else(|| Error::Custom("Vector length does not match bitmap".to_owned()))?;
            *e = Some(<T>::from(value.clone()));
        }
    }

    Ok(r_arr)
}

/// Convert Link node into vector of Cids
fn cids_from_links<V>(links: &[Option<Link<V>>; WIDTH]) -> Result<Vec<Cid>, Error> {
    links
        .iter()
        .filter_map(|c| match c {
            Some(Link::Cid(cid)) => Some(Ok(cid.clone())),
            Some(Link::Cached(_)) => Some(Err(Error::Cached)),
            None => None,
        })
        .collect()
}

impl<V> Serialize for Node<V>
where
    V: Clone + Serialize,
{
    fn serialize<S>(&self, s: S) -> Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        match &self {
            Node::Leaf { bmap, vals } => (bmap, [0u8; 0], values_to_vec(*bmap, &vals)).serialize(s),
            Node::Link { bmap, links } => {
                let cids = cids_from_links(links).map_err(|e| ser::Error::custom(e.to_string()))?;
                (bmap, cids, [0u8; 0]).serialize(s)
            }
        }
    }
}

impl<'de, V> Deserialize<'de> for Node<V>
where
    V: Deserialize<'de> + Clone,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        let (bmap, links, values): (BitMap, Vec<Cid>, Vec<V>) =
            Deserialize::deserialize(deserializer)?;

        if links.is_empty() {
            Ok(Self::Leaf {
                bmap,
                vals: vec_to_values(bmap, values).map_err(|e| de::Error::custom(e.to_string()))?,
            })
        } else {
            Ok(Self::Link {
                bmap,
                links: vec_to_values(bmap, links).map_err(|e| de::Error::custom(e.to_string()))?,
            })
        }
    }
}

impl<V> Node<V>
where
    V: Clone + DeserializeOwned + Serialize,
{
    /// Flushes cache for node, replacing any cached values with a Cid variant
    pub(super) fn flush<DB: BlockStore>(&mut self, bs: &DB) -> Result<(), Error> {
        if let Node::Link { links, .. } = self {
            for link in &mut links.iter_mut() {
                if let Some(Link::Cached(n)) = link {
                    // flush sub node to clear caches
                    n.flush(bs)?;

                    // Puts node in blockstore and and retrieves it's CID
                    let cid = bs.put(n)?;

                    // Turn cached node into a Cid link
                    *link = Some(Link::Cid(cid));
                }
            }
        }

        Ok(())
    }

    pub(super) fn bitmap(&self) -> &BitMap {
        match self {
            Node::Link { bmap, .. } => bmap,
            Node::Leaf { bmap, .. } => bmap,
        }
    }

    /// Check if node is empty
    pub(super) fn empty(&self) -> bool {
        self.bitmap().is_empty()
    }

    /// Gets value at given index of AMT given height
    pub(super) fn get<DB: BlockStore>(
        &self,
        bs: &DB,
        height: u32,
        i: u64,
    ) -> Result<Option<V>, Error> {
        let sub_i = i / nodes_for_height(height);
        if !self.bitmap().get_bit(sub_i) {
            return Ok(None);
        }

        match self {
            Node::Leaf { vals, .. } => Ok(vals[i as usize].clone()),
            Node::Link { links, .. } => match &links[sub_i as usize] {
                Some(Link::Cid(cid)) => {
                    // TODO after benchmarking check if cache should be updated from get
                    let node: Node<V> = bs.get::<Node<V>>(cid)?.ok_or_else(|| {
                        Error::Cid("Cid did not match any in database".to_owned())
                    })?;

                    // Get from node pulled into memory from Cid
                    node.get(bs, height - 1, i % nodes_for_height(height))
                }
                Some(Link::Cached(n)) => n.get(bs, height - 1, i % nodes_for_height(height)),
                None => Ok(None),
            },
        }
    }

    /// Set value in node
    pub(super) fn set<DB: BlockStore>(
        &mut self,
        bs: &DB,
        height: u32,
        i: u64,
        val: V,
    ) -> Result<bool, Error> {
        if height == 0 {
            return Ok(self.set_leaf(i, val));
        }

        let nfh = nodes_for_height(height);

        // If dividing by nodes for height should give an index for link in node
        let idx: usize = (i / nfh) as usize;
        assert!(idx < 8);

        if let Node::Link { links, bmap } = self {
            links[idx] = match &mut links[idx] {
                Some(Link::Cid(cid)) => {
                    let node = bs.get::<Node<V>>(cid)?.ok_or_else(|| {
                        Error::Cid("Cid did not match any in database".to_owned())
                    })?;

                    Some(Link::Cached(Box::new(node)))
                }
                None => {
                    let node = match height {
                        1 => Node::Leaf {
                            bmap: Default::default(),
                            vals: Default::default(),
                        },
                        _ => Node::Link {
                            bmap: Default::default(),
                            links: Default::default(),
                        },
                    };
                    bmap.set_bit(idx as u64);
                    Some(Link::Cached(Box::new(node)))
                }
                Some(Link::Cached(node)) => return node.set(bs, height - 1, i % nfh, val),
            };

            if let Some(Link::Cached(n)) = &mut links[idx] {
                n.set(bs, height - 1, i % nfh, val)
            } else {
                unreachable!("Value is set as cached")
            }
        } else {
            unreachable!("Non zero height in AMT is always Links type")
        }
    }

    fn set_leaf(&mut self, i: u64, val: V) -> bool {
        let already_set = self.bitmap().get_bit(i);

        match self {
            Node::Leaf { vals, bmap } => {
                vals[i as usize] = Some(val);
                bmap.set_bit(i);
                !already_set
            }
            Node::Link { .. } => panic!("set_leaf should never be called on a shard of links"),
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

        if !self.bitmap().get_bit(sub_i) {
            // Value does not exist in AMT
            return Ok(false);
        }

        match self {
            Self::Leaf { bmap, .. } => {
                assert_eq!(
                    height, 0,
                    "Height must be 0 when clearing bit for leaf node"
                );

                // When deleting from node, should only need to clear bit from bitmap
                bmap.clear_bit(i);
                Ok(true)
            }
            Self::Link { links, bmap } => {
                let mut sub_node: Node<V> = match &links[sub_i as usize] {
                    Some(Link::Cached(n)) => *n.clone(),
                    Some(Link::Cid(cid)) => bs.get(cid)?.ok_or_else(|| {
                        Error::Cid("Cid did not match any in database".to_owned())
                    })?,
                    None => unreachable!("Bitmap value for index is set"),
                };

                // Follow node to delete from subnode
                if !sub_node.delete(bs, height - 1, i % nodes_for_height(height))? {
                    // Index to be deleted was not found
                    return Ok(false);
                }

                // Value was deleted, move node to cache or clear bit if removing shard
                links[sub_i as usize] = if sub_node.bitmap().is_empty() {
                    bmap.clear_bit(sub_i);
                    None
                } else {
                    Some(Link::Cached(Box::new(sub_node)))
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
        assert_eq!(from_slice::<Node<u8>>(&nbz).unwrap(), node);
    }
}
