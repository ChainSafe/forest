// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::{nodes_for_height, BitMap, Error, WIDTH};
use cid::{Cid, Code::Blake2b256};
use encoding::{
    de::{self, Deserialize, DeserializeOwned},
    ser::{self, Serialize},
};
use ipld_blockstore::BlockStore;
use once_cell::unsync::OnceCell;
use std::error::Error as StdError;

use super::ValueMut;

/// This represents a link to another Node
#[derive(Debug)]
pub(super) enum Link<V> {
    /// Unchanged link to data with an atomic cache.
    Cid {
        cid: Cid,
        cache: OnceCell<Box<Node<V>>>,
    },
    /// Modifications have been made to the link, requires flush to clear
    Dirty(Box<Node<V>>),
}

impl<V> PartialEq for Link<V>
where
    V: PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (&Link::Cid { cid: ref a, .. }, &Link::Cid { cid: ref b, .. }) => a == b,
            (&Link::Dirty(ref a), &Link::Dirty(ref b)) => a == b,
            _ => false,
        }
    }
}

impl<V> Eq for Link<V> where V: Eq {}

impl<V> From<Cid> for Link<V> {
    fn from(cid: Cid) -> Link<V> {
        Link::Cid {
            cid,
            cache: Default::default(),
        }
    }
}

/// Node represents either a shard of values in the form of bytes or links to other nodes
#[derive(PartialEq, Eq, Debug)]
#[allow(clippy::large_enum_variant)]
pub(super) enum Node<V> {
    /// Node is a link node, contains array of Cid or cached sub nodes.
    Link {
        bmap: BitMap,
        links: [Option<Link<V>>; WIDTH],
    },
    /// Leaf node, this array contains only values.
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
fn values_to_vec<T>(values: &[Option<T>]) -> Vec<&T> {
    values.iter().filter_map(|val| val.as_ref()).collect()
}

/// Puts values from vector into shard array
fn vec_to_values<V, T>(bmap: BitMap, values: Vec<V>) -> Result<[Option<T>; WIDTH], Error>
where
    T: From<V>,
{
    let mut r_arr: [Option<T>; WIDTH] = Default::default();

    let mut v_iter = values.into_iter();

    for (i, e) in (0..).zip(r_arr.iter_mut()) {
        if bmap.get_bit(i) {
            let value = v_iter.next().ok_or(Error::InvalidVecLength)?;
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
            Some(Link::Cid { cid, .. }) => Some(Ok(cid)),
            Some(Link::Dirty(_)) => Some(Err(Error::Cached)),
            None => None,
        })
        .collect()
}

impl<V> Serialize for Node<V>
where
    V: Serialize,
{
    fn serialize<S>(&self, s: S) -> Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        match &self {
            Node::Leaf { bmap, vals } => {
                (bmap, [0u8; 0], values_to_vec(vals.as_ref())).serialize(s)
            }
            Node::Link { bmap, links } => {
                let cids = cids_from_links(links).map_err(|e| ser::Error::custom(e.to_string()))?;
                (bmap, cids, [0u8; 0]).serialize(s)
            }
        }
    }
}

impl<'de, V> Deserialize<'de> for Node<V>
where
    V: Deserialize<'de>,
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
    V: DeserializeOwned + Serialize,
{
    /// Flushes cache for node, replacing any cached values with a Cid variant
    pub(super) fn flush<DB: BlockStore>(&mut self, bs: &DB) -> Result<(), Error> {
        if let Node::Link { links, bmap } = self {
            for (i, link) in (0..).zip(links.iter_mut()) {
                // links should only be flushed if the bitmap is set.
                if bmap.get_bit(i) {
                    #[cfg(feature = "go-interop")]
                    if let Some(Link::Cid { cache, .. }) = link {
                        // Yes, this is necessary to interop, and yes this is safe to borrow
                        // mutably because there are no values changed here, just extra db writes.
                        if let Some(cached) = cache.get_mut() {
                            cached.flush(bs)?;
                            bs.put(cached, Blake2b256)?;
                        }
                    }

                    if let Some(Link::Dirty(n)) = link {
                        // flush sub node to clear caches
                        n.flush(bs)?;

                        // Puts node in blockstore and and retrieves it's CID
                        let cid = bs.put(n, Blake2b256)?;

                        // Can keep the flushed node in link cache
                        let node = std::mem::take(n);
                        let cache = OnceCell::from(node);

                        // Turn dirty node into a Cid link
                        *link = Some(Link::Cid { cid, cache });
                    }
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
    ) -> Result<Option<&V>, Error> {
        let sub_i = i / nodes_for_height(height);
        if !self.bitmap().get_bit(sub_i) {
            return Ok(None);
        }

        match self {
            Node::Leaf { vals, .. } => Ok(vals[i as usize].as_ref()),
            Node::Link { links, .. } => match &links[sub_i as usize] {
                Some(Link::Cid { cid, cache }) => {
                    let cached_node =
                        cache.get_or_try_init(|| -> Result<Box<Node<V>>, Error> {
                            bs.get(cid)?
                                .ok_or_else(|| Error::CidNotFound(cid.to_string()))
                        })?;

                    cached_node.get(bs, height - 1, i % nodes_for_height(height))
                }
                Some(Link::Dirty(n)) => n.get(bs, height - 1, i % nodes_for_height(height)),
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
                Some(Link::Cid { cid, cache }) => {
                    let cache_node = std::mem::take(cache);
                    let sub_node = if let Some(sn) = cache_node.into_inner() {
                        sn
                    } else {
                        // Only retrieve sub node if not found in cache
                        bs.get(&cid)?.ok_or(Error::RootNotFound)?
                    };

                    Some(Link::Dirty(sub_node))
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
                    Some(Link::Dirty(Box::new(node)))
                }
                Some(Link::Dirty(node)) => return node.set(bs, height - 1, i % nfh, val),
            };

            if let Some(Link::Dirty(n)) = &mut links[idx] {
                n.set(bs, height - 1, i % nfh, val)
            } else {
                unreachable!("Value is set as cached")
            }
        } else {
            // ! This should not be handled, but there is a bug in the go implementation
            // ! and this needs to be matched
            *self = Node::Link {
                links: Default::default(),
                bmap: Default::default(),
            };
            self.set(bs, height, i, val)
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
            Self::Leaf { bmap, vals } => {
                assert_eq!(
                    height, 0,
                    "Height must be 0 when clearing bit for leaf node"
                );

                bmap.clear_bit(i);
                vals[i as usize] = None;
                Ok(true)
            }
            Self::Link { links, bmap } => {
                match &mut links[sub_i as usize] {
                    mod_link @ Some(Link::Dirty(_)) => {
                        let mut remove = false;
                        if let Some(Link::Dirty(n)) = mod_link {
                            if !n.delete(bs, height - 1, i % nodes_for_height(height))? {
                                // Index to be deleted was not found
                                return Ok(false);
                            }
                            if n.bitmap().is_empty() {
                                bmap.clear_bit(sub_i);
                                remove = true;
                            }
                        } else {
                            unreachable!("variant matched specifically");
                        }

                        // Remove needs to be done outside of the `if let` for memory safety.
                        if remove {
                            *mod_link = None;
                        }
                    }
                    cid_link @ Some(Link::Cid { .. }) => {
                        let sub_node = if let Some(Link::Cid { cid, cache }) = cid_link {
                            // Take cache, will be replaced if no nodes deleted
                            let cache_node = std::mem::take(cache);
                            let mut sub_node = if let Some(sn) = cache_node.into_inner() {
                                sn
                            } else {
                                // Only retrieve sub node if not found in cache
                                bs.get(&cid)?.ok_or(Error::RootNotFound)?
                            };
                            if !sub_node.delete(bs, height - 1, i % nodes_for_height(height))? {
                                // Replace cache, no node deleted.
                                // Error can be ignored because value will always be the same
                                // even if possible to hit race condition.
                                let _ = cache.set(sub_node);

                                // Index to be deleted was not found
                                return Ok(false);
                            }
                            sub_node
                        } else {
                            unreachable!("variant matched specifically");
                        };

                        if sub_node.bitmap().is_empty() {
                            // Sub node is empty, clear bit and current link node.
                            bmap.clear_bit(sub_i);
                            *cid_link = None;
                        } else {
                            *cid_link = Some(Link::Dirty(sub_node));
                        }
                    }
                    None => unreachable!("Bitmap value for index is set"),
                };

                Ok(true)
            }
        }
    }

    pub(super) fn for_each_while<S, F>(
        &self,
        store: &S,
        height: u64,
        offset: u64,
        f: &mut F,
    ) -> Result<bool, Box<dyn StdError>>
    where
        F: FnMut(u64, &V) -> Result<bool, Box<dyn StdError>>,
        S: BlockStore,
    {
        match self {
            Node::Leaf { bmap, vals } => {
                for (i, v) in (0..).zip(vals.iter()) {
                    if bmap.get_bit(i) {
                        let keep_going = f(
                            offset + i,
                            v.as_ref().expect("set bit should contain value"),
                        )?;

                        if !keep_going {
                            return Ok(false);
                        }
                    }
                }
            }
            Node::Link { bmap, links } => {
                for (i, l) in (0..).zip(links.iter()) {
                    if bmap.get_bit(i) {
                        let offs = offset + (i * nodes_for_height(height));
                        let keep_going = match l.as_ref().expect("bit set at index") {
                            Link::Dirty(sub) => sub.for_each_while(store, height - 1, offs, f)?,
                            Link::Cid { cid, cache } => {
                                // TODO simplify with try_init when go-interop feature not needed
                                if let Some(cached_node) = cache.get() {
                                    cached_node.for_each_while(store, height - 1, offs, f)?
                                } else {
                                    let node: Box<Node<V>> = store
                                        .get(cid)?
                                        .ok_or_else(|| Error::CidNotFound(cid.to_string()))?;

                                    #[cfg(not(feature = "go-interop"))]
                                    {
                                        // Ignore error intentionally, the cache value will always be the same
                                        let cache_node = cache.get_or_init(|| node);
                                        cache_node.for_each_while(store, height - 1, offs, f)?
                                    }

                                    #[cfg(feature = "go-interop")]
                                    node.for_each_while(store, height - 1, offs, f)?
                                }
                            }
                        };

                        if !keep_going {
                            return Ok(false);
                        }
                    }
                }
            }
        }

        Ok(true)
    }

    /// Returns a `(keep_going, did_mutate)` pair. `keep_going` will be `false` iff
    /// a closure call returned `Ok(false)`, indicating that a `break` has happened.
    /// `did_mutate` will be `true` iff any of the values in the node was actually
    /// mutated inside the closure, requiring the node to be cached.
    pub(super) fn for_each_while_mut<S, F>(
        &mut self,
        store: &S,
        height: u64,
        offset: u64,
        f: &mut F,
    ) -> Result<(bool, bool), Box<dyn StdError>>
    where
        F: FnMut(u64, &mut ValueMut<'_, V>) -> Result<bool, Box<dyn StdError>>,
        S: BlockStore,
    {
        let mut did_mutate = false;

        match self {
            Node::Leaf { bmap, vals } => {
                for (i, v) in (0..).zip(vals.iter_mut()) {
                    if bmap.get_bit(i) {
                        let mut value_mut =
                            ValueMut::new(v.as_mut().expect("set bit should contain value"));

                        let keep_going = f(offset + i, &mut value_mut)?;
                        did_mutate |= value_mut.value_changed();

                        if !keep_going {
                            return Ok((false, did_mutate));
                        }
                    }
                }
            }
            Node::Link { bmap, links } => {
                for (i, l) in (0..).zip(links.iter_mut()) {
                    if bmap.get_bit(i) {
                        let offs = offset + (i * nodes_for_height(height));
                        let link = l.as_mut().expect("bit set at index");
                        let (keep_going, did_mutate_node) = match link {
                            Link::Dirty(sub) => {
                                sub.for_each_while_mut(store, height - 1, offs, f)?
                            }
                            Link::Cid { cid, cache } => {
                                let cache_node = std::mem::take(cache);

                                #[allow(unused_variables)]
                                let (mut node, cached) = if let Some(sn) = cache_node.into_inner() {
                                    (sn, true)
                                } else {
                                    // Only retrieve sub node if not found in cache
                                    (store.get(&cid)?.ok_or(Error::RootNotFound)?, false)
                                };

                                let (keep_going, did_mutate_node) =
                                    node.for_each_while_mut(store, height - 1, offs, f)?;

                                if did_mutate_node {
                                    *link = Link::Dirty(node);
                                } else {
                                    #[cfg(feature = "go-interop")]
                                    {
                                        if cached {
                                            let _ = cache.set(node);
                                        }
                                    }

                                    // Replace cache, or else iteration over without modification
                                    // will consume cache
                                    #[cfg(not(feature = "go-interop"))]
                                    let _ = cache.set(node);
                                }

                                (keep_going, did_mutate_node)
                            }
                        };

                        did_mutate |= did_mutate_node;

                        if !keep_going {
                            return Ok((false, did_mutate));
                        }
                    }
                }
            }
        }

        Ok((true, did_mutate))
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
