// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::{nodes_for_height, BitMap, Error, WIDTH};
use cid::{multihash::Blake2b256, Cid};
use encoding::{
    de::{self, Deserialize, DeserializeOwned},
    ser::{self, Serialize},
};
use ipld_blockstore::BlockStore;
use std::error::Error as StdError;

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
fn values_to_vec<T>(bmap: BitMap, values: &[Option<T>; WIDTH]) -> Vec<&T>
where
    T: Clone,
{
    let mut v: Vec<&T> = Vec::new();
    for (i, _) in values.iter().enumerate().take(WIDTH) {
        if bmap.get_bit(i as u64) {
            v.push(values[i].as_ref().unwrap())
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

    let mut v_iter = values.into_iter();

    for (i, e) in (0..).zip(r_arr.iter_mut()) {
        if bmap.get_bit(i) {
            let value = v_iter.next().ok_or_else(|| Error::InvalidVecLength)?;
            *e = Some(<T>::from(value));
        }
    }

    Ok(r_arr)
}

/// Convert Link node into vector of Cids
fn cids_from_links<V>(links: &[Option<Link<V>>; WIDTH]) -> Result<Vec<&Cid>, Error> {
    links
        .iter()
        .filter_map(|c| match c {
            Some(Link::Cid(cid)) => Some(Ok(cid)),
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
                    let cid = bs.put(n, Blake2b256)?;

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

    /// Gets value at given index of Amt given height
    pub(super) fn get<DB: BlockStore>(
        &self,
        bs: &DB,
        height: u64,
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
                    let node: Node<V> = bs
                        .get::<Node<V>>(cid)?
                        .ok_or_else(|| Error::CidNotFound(cid.to_string()))?;

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
        height: u64,
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
                    let node = bs.get::<Node<V>>(cid)?.ok_or_else(|| Error::RootNotFound)?;

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
            unreachable!("Non zero height in Amt is always Links type")
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

    /// Delete value in Amt by index
    pub(super) fn delete<DB: BlockStore>(
        &mut self,
        bs: &DB,
        height: u64,
        i: u64,
    ) -> Result<bool, Error> {
        let sub_i = i / nodes_for_height(height);

        if !self.bitmap().get_bit(sub_i) {
            // Value does not exist in Amt
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
                let mut sub_node: Box<Node<V>> = match links[sub_i as usize].take() {
                    Some(Link::Cached(n)) => n,
                    Some(Link::Cid(cid)) => bs.get(&cid)?.ok_or_else(|| Error::RootNotFound)?,
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
                    Some(Link::Cached(sub_node))
                };

                Ok(true)
            }
        }
    }

    pub(super) fn for_each<S, F>(
        &self,
        store: &S,
        height: u64,
        offset: u64,
        f: &mut F,
    ) -> Result<(), Box<dyn StdError>>
    where
        F: FnMut(u64, &V) -> Result<(), Box<dyn StdError>>,
        S: BlockStore,
    {
        match self {
            Node::Leaf { bmap, vals } => {
                for (i, v) in (0..).zip(vals.iter()) {
                    if bmap.get_bit(i) {
                        f(
                            offset + i,
                            v.as_ref().expect("set bit should contain value"),
                        )?;
                    }
                }
            }
            Node::Link { bmap, links } => {
                for (i, l) in (0..).zip(links.iter()) {
                    if bmap.get_bit(i) {
                        let offs = offset + (i * nodes_for_height(height));
                        match l.as_ref().expect("bit set at index") {
                            Link::Cached(sub) => sub.for_each(store, height - 1, offs, f)?,
                            Link::Cid(cid) => {
                                let node = store
                                    .get::<Node<V>>(cid)
                                    .map_err(|e| e.to_string())?
                                    .ok_or_else(|| Error::RootNotFound)?;

                                node.for_each(store, height - 1, offs, f)?;
                            }
                        }
                    }
                }
            }
        }
        Ok(())
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
