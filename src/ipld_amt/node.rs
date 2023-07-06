// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
// Copyright 2021-2023 Protocol Labs
// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::convert::{TryFrom, TryInto};

use anyhow::anyhow;
use cid::multihash::Code;
use cid::Cid;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding3::{strict_bytes, BytesSer, CborStore};
use once_cell::unsync::OnceCell;
use serde::de::{self, DeserializeOwned};
use serde::{ser, Deserialize, Serialize};

use super::ValueMut;
use super::{bmap_bytes, init_sized_vec, nodes_for_height, Error};

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

impl<'de, V> Deserialize<'de> for Link<V>
where
    V: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        let cid: Cid = Deserialize::deserialize(deserializer)?;
        Ok(Link::Cid {
            cid,
            cache: Default::default(),
        })
    }
}

impl<V> PartialEq for Link<V>
where
    V: PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Link::Cid { cid: a, .. }, Link::Cid { cid: b, .. }) => a == b,
            (Link::Dirty(a), Link::Dirty(b)) => a == b,
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
    Link { links: Vec<Option<Link<V>>> },
    /// Leaf node, this array contains only values.
    Leaf { vals: Vec<Option<V>> },
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
            Node::Leaf { vals } => {
                let mut values = Vec::<&V>::with_capacity(vals.len());
                let mut bmap = vec![0u8; ((vals.len().saturating_sub(1)) / 8) + 1];
                for (i, v) in vals.iter().enumerate() {
                    if let Some(val) = v {
                        values.push(val);
                        bmap[i / 8] |= 1 << (i % 8);
                    }
                }
                (BytesSer(&bmap), Vec::<&Cid>::new(), values).serialize(s)
            }
            Node::Link { links } => {
                let mut collapsed = Vec::<&Cid>::with_capacity(links.len());
                let mut bmap = vec![0u8; ((links.len().saturating_sub(1)) / 8) + 1];
                for (i, v) in links.iter().enumerate() {
                    if let Some(val) = v {
                        if let Link::Cid { cid, .. } = val {
                            collapsed.push(cid);
                            bmap[i / 8] |= 1 << (i % 8);
                        } else {
                            return Err(ser::Error::custom(Error::Cached));
                        }
                    }
                }
                (BytesSer(&bmap), collapsed, Vec::<&V>::new()).serialize(s)
            }
        }
    }
}

#[derive(Serialize, Deserialize)]
pub(super) struct CollapsedNode<V>(#[serde(with = "strict_bytes")] Vec<u8>, Vec<Cid>, Vec<V>);

impl<V> CollapsedNode<V> {
    pub(super) fn expand(self, bit_width: u32) -> Result<Node<V>, Error> {
        let CollapsedNode(bmap, links, values) = self;
        if !links.is_empty() && !values.is_empty() {
            return Err(Error::LinksAndValues);
        }

        if bmap_bytes(bit_width) != bmap.len() {
            return Err(anyhow!(
                "expected bitfield of length {}, found bitfield with length {}",
                bmap_bytes(bit_width),
                bmap.len()
            )
            .into());
        }

        if !links.is_empty() {
            let mut links_iter = links.into_iter();
            let mut links = init_sized_vec::<Link<V>>(bit_width);
            for (i, v) in links.iter_mut().enumerate() {
                if bmap[i / 8] & (1 << (i % 8)) != 0 {
                    *v = Some(Link::from(links_iter.next().ok_or_else(|| {
                        anyhow!("Bitmap contained more set bits than links provided",)
                    })?))
                }
            }
            if links_iter.next().is_some() {
                return Err(anyhow!("Bitmap contained less set bits than links provided",).into());
            }
            Ok(Node::Link { links })
        } else {
            let mut val_iter = values.into_iter();
            let mut vals = init_sized_vec::<V>(bit_width);
            for (i, v) in vals.iter_mut().enumerate() {
                if bmap[i / 8] & (1 << (i % 8)) != 0 {
                    *v = Some(val_iter.next().ok_or_else(|| {
                        anyhow!("Bitmap contained more set bits than values provided")
                    })?)
                }
            }
            if val_iter.next().is_some() {
                return Err(anyhow!("Bitmap contained less set bits than values provided").into());
            }
            Ok(Node::Leaf { vals })
        }
    }
}

impl<V> Node<V>
where
    V: Serialize + DeserializeOwned,
{
    /// Empty node. This is an invalid format and should only be used temporarily to avoid
    /// allocations.
    pub(super) fn empty() -> Self {
        Node::Leaf {
            vals: Default::default(),
        }
    }

    /// Flushes cache for node, replacing any cached values with a Cid variant
    pub(super) fn flush<DB: Blockstore>(&mut self, bs: &DB) -> Result<(), Error> {
        if let Node::Link { links } = self {
            for link in links.iter_mut().flatten() {
                // links should only be flushed if the bitmap is set.
                if let Link::Dirty(n) = link {
                    // flush sub node to clear caches
                    n.flush(bs)?;

                    // Puts node in blockstore and and retrieves it's CID
                    let cid = bs.put_cbor(n, Code::Blake2b256)?;

                    // Replace the data with some arbitrary node to move without requiring clone
                    let existing = std::mem::replace(n, Box::new(Node::empty()));

                    // Can keep the flushed node in link cache
                    let cache = OnceCell::from(existing);
                    *link = Link::Cid { cid, cache };
                }
            }
        }

        Ok(())
    }

    /// Returns true if there is only a link in the first index of the values.
    /// This node can be collapsed into the parent node.
    pub(super) fn can_collapse(&self) -> bool {
        match self {
            Node::Link { links } => {
                // Check if first index is a link and all other values are empty.
                links.get(0).and_then(|l| l.as_ref()).is_some()
                    && links
                        .get(1..)
                        .map(|l| l.iter().all(|l| l.is_none()))
                        .unwrap_or(true)
            }
            Node::Leaf { .. } => false,
        }
    }

    /// Returns true if there are no values in the node.
    pub(super) fn is_empty(&self) -> bool {
        match self {
            Node::Link { links } => links.iter().all(|l| l.is_none()),
            Node::Leaf { vals } => vals.iter().all(|l| l.is_none()),
        }
    }

    /// Gets value at given index of Amt given height
    pub(super) fn get<DB: Blockstore>(
        &self,
        bs: &DB,
        height: u32,
        bit_width: u32,
        i: u64,
    ) -> Result<Option<&V>, Error> {
        match self {
            Node::Leaf { vals, .. } => Ok(vals.get(i as usize).and_then(|v| v.as_ref())),
            Node::Link { links, .. } => {
                let sub_i: usize = (i / nodes_for_height(bit_width, height))
                    .try_into()
                    .unwrap();
                match links.get(sub_i).and_then(|v| v.as_ref()) {
                    Some(Link::Cid { cid, cache }) => {
                        let cached_node = cache.get_or_try_init(|| {
                            bs.get_cbor::<CollapsedNode<V>>(cid)?
                                .ok_or_else(|| Error::CidNotFound(cid.to_string()))?
                                .expand(bit_width)
                                .map(Box::new)
                        })?;

                        cached_node.get(
                            bs,
                            height - 1,
                            bit_width,
                            i % nodes_for_height(bit_width, height),
                        )
                    }
                    Some(Link::Dirty(n)) => n.get(
                        bs,
                        height - 1,
                        bit_width,
                        i % nodes_for_height(bit_width, height),
                    ),
                    None => Ok(None),
                }
            }
        }
    }

    /// Set value in node
    pub(super) fn set<DB: Blockstore>(
        &mut self,
        bs: &DB,
        height: u32,
        bit_width: u32,
        i: u64,
        val: V,
    ) -> Result<Option<V>, Error> {
        if height == 0 {
            return Ok(self.set_leaf(i, val));
        }

        let nfh = nodes_for_height(bit_width, height);

        // If dividing by nodes for height should give an index for link in node
        let idx: usize = (i / nfh).try_into().expect("index overflow");

        if let Node::Link { links } = self {
            links[idx] = match &mut links[idx] {
                Some(Link::Cid { cid, cache }) => {
                    let cache_node = std::mem::take(cache);
                    let sub_node = if let Some(sn) = cache_node.into_inner() {
                        sn
                    } else {
                        // Only retrieve sub node if not found in cache
                        bs.get_cbor::<CollapsedNode<V>>(cid)?
                            .ok_or_else(|| Error::CidNotFound(cid.to_string()))?
                            .expand(bit_width)
                            .map(Box::new)?
                    };

                    Some(Link::Dirty(sub_node))
                }
                None => {
                    let node = match height {
                        1 => Node::Leaf {
                            vals: init_sized_vec(bit_width),
                        },
                        _ => Node::Link {
                            links: init_sized_vec(bit_width),
                        },
                    };
                    Some(Link::Dirty(Box::new(node)))
                }
                Some(Link::Dirty(node)) => {
                    return node.set(bs, height - 1, bit_width, i % nfh, val)
                }
            };

            if let Some(Link::Dirty(n)) = &mut links[idx] {
                n.set(bs, height - 1, bit_width, i % nfh, val)
            } else {
                unreachable!("Value is set as cached")
            }
        } else {
            unreachable!("should not be handled");
        }
    }

    fn set_leaf(&mut self, i: u64, val: V) -> Option<V> {
        match self {
            Node::Leaf { vals } => {
                let prev = std::mem::replace(
                    vals.get_mut(usize::try_from(i).unwrap()).unwrap(),
                    Some(val),
                );
                prev
            }
            Node::Link { .. } => panic!("set_leaf should never be called on a shard of links"),
        }
    }

    /// Delete value in Amt by index
    pub(super) fn delete<DB: Blockstore>(
        &mut self,
        bs: &DB,
        height: u32,
        bit_width: u32,
        i: u64,
    ) -> Result<Option<V>, Error> {
        match self {
            Self::Leaf { vals } => Ok(vals
                .get_mut(usize::try_from(i).unwrap())
                .and_then(std::mem::take)),
            Self::Link { links } => {
                let sub_i: usize = (i / nodes_for_height(bit_width, height))
                    .try_into()
                    .unwrap();
                let (deleted, replace) = match &mut links[sub_i] {
                    Some(Link::Dirty(n)) => {
                        let deleted = n.delete(
                            bs,
                            height - 1,
                            bit_width,
                            i % nodes_for_height(bit_width, height),
                        )?;
                        if deleted.is_none() {
                            // Index to be deleted was not found
                            return Ok(None);
                        }
                        if !n.is_empty() {
                            // Link node is not empty yet, just return deleted
                            return Ok(deleted);
                        }

                        // Remove needs to be done outside of the `if let` for memory safety.
                        (deleted, None)
                    }
                    Some(Link::Cid { cid, cache }) => {
                        // Take cache, will be replaced if no nodes deleted
                        cache.get_or_try_init(|| {
                            bs.get_cbor::<CollapsedNode<V>>(cid)?
                                .ok_or_else(|| Error::CidNotFound(cid.to_string()))?
                                .expand(bit_width)
                                .map(Box::new)
                        })?;
                        let sub_node = cache.get_mut().expect("filled line above");
                        let deleted = sub_node.delete(
                            bs,
                            height - 1,
                            bit_width,
                            i % nodes_for_height(bit_width, height),
                        )?;
                        if deleted.is_none() {
                            // Index to be deleted was not found
                            return Ok(None);
                        };
                        let sub_node = std::mem::replace(sub_node, Box::new(Node::empty()));

                        if sub_node.is_empty() {
                            // Sub node is empty, clear link.
                            (deleted, None)
                        } else {
                            // Link was modified and is now marked dirty.
                            (deleted, Some(Link::Dirty(sub_node)))
                        }
                    }
                    // Link index is empty.
                    None => return Ok(None),
                };

                links[sub_i] = replace;

                Ok(deleted)
            }
        }
    }

    pub(super) fn for_each_while<S, F>(
        &self,
        bs: &S,
        height: u32,
        bit_width: u32,
        offset: u64,
        f: &mut F,
    ) -> Result<bool, Error>
    where
        F: FnMut(u64, &V) -> anyhow::Result<bool>,
        S: Blockstore,
    {
        match self {
            Node::Leaf { vals } => {
                for (i, v) in (0..).zip(vals.iter()) {
                    if let Some(v) = v {
                        let keep_going = f(offset + i, v)?;

                        if !keep_going {
                            return Ok(false);
                        }
                    }
                }
            }
            Node::Link { links } => {
                for (i, l) in (0..).zip(links.iter()) {
                    if let Some(l) = l {
                        let offs = offset + (i * nodes_for_height(bit_width, height));
                        let keep_going = match l {
                            Link::Dirty(sub) => {
                                sub.for_each_while(bs, height - 1, bit_width, offs, f)?
                            }
                            Link::Cid { cid, cache } => {
                                let cached_node = cache.get_or_try_init(|| {
                                    bs.get_cbor::<CollapsedNode<V>>(cid)?
                                        .ok_or_else(|| Error::CidNotFound(cid.to_string()))?
                                        .expand(bit_width)
                                        .map(Box::new)
                                })?;

                                cached_node.for_each_while(bs, height - 1, bit_width, offs, f)?
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

    /// Returns a `(keep_going, did_mutate)` pair. `keep_going` will be `false` if
    /// a closure call returned `Ok(false)`, indicating that a `break` has happened.
    /// `did_mutate` will be `true` if any of the values in the node was actually
    /// mutated inside the closure, requiring the node to be cached.
    pub(super) fn for_each_while_mut<S, F>(
        &mut self,
        bs: &S,
        height: u32,
        bit_width: u32,
        offset: u64,
        f: &mut F,
    ) -> Result<(bool, bool), Error>
    where
        F: FnMut(u64, &mut ValueMut<'_, V>) -> anyhow::Result<bool>,
        S: Blockstore,
    {
        let mut did_mutate = false;

        match self {
            Node::Leaf { vals } => {
                for (i, v) in (0..).zip(vals.iter_mut()) {
                    if let Some(v) = v {
                        let mut value_mut = ValueMut::new(v);

                        let keep_going = f(offset + i, &mut value_mut)?;
                        did_mutate |= value_mut.value_changed();

                        if !keep_going {
                            return Ok((false, did_mutate));
                        }
                    }
                }
            }
            Node::Link { links } => {
                for (i, l) in (0..).zip(links.iter_mut()) {
                    if let Some(link) = l {
                        let offs = offset + (i * nodes_for_height(bit_width, height));
                        let (keep_going, did_mutate_node) = match link {
                            Link::Dirty(sub) => {
                                sub.for_each_while_mut(bs, height - 1, bit_width, offs, f)?
                            }
                            Link::Cid { cid, cache } => {
                                cache.get_or_try_init(|| {
                                    bs.get_cbor::<CollapsedNode<V>>(cid)?
                                        .ok_or_else(|| Error::CidNotFound(cid.to_string()))?
                                        .expand(bit_width)
                                        .map(Box::new)
                                })?;
                                let node = cache.get_mut().expect("cache filled on line above");

                                let (keep_going, did_mutate_node) =
                                    node.for_each_while_mut(bs, height - 1, bit_width, offs, f)?;

                                if did_mutate_node {
                                    // Cache was mutated, switch it to dirty
                                    *link = Link::Dirty(std::mem::replace(
                                        node,
                                        Box::new(Node::empty()),
                                    ));
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

    /// Iterates through the current node in the tree and all sub-trees. `start_at` refers to the
    /// global AMT index, before which no values should be traversed and `limit` is the maximum
    /// number of leaf nodes that should be traversed in this sub-tree. `offset` refers the offset
    /// in the global AMT address space that this sub-tree is rooted at.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn for_each_while_ranged<S, F>(
        &self,
        bs: &S,
        start_at: Option<u64>,
        limit: Option<u64>,
        height: u32,
        bit_width: u32,
        offset: u64,
        f: &mut F,
    ) -> Result<(bool, u64, Option<u64>), Error>
    where
        F: FnMut(u64, &V) -> anyhow::Result<bool>,
        S: Blockstore,
    {
        let mut traversed_count = 0_u64;
        match self {
            Node::Leaf { vals } => {
                let start_idx = start_at.map_or(0, |s| s.saturating_sub(offset));
                let mut keep_going = true;
                for (i, v) in (start_idx..).zip(vals[start_idx as usize..].iter()) {
                    let idx = offset + i;
                    if let Some(v) = v {
                        if limit.map_or(false, |l| traversed_count >= l) {
                            return Ok((keep_going, traversed_count, Some(idx)));
                        } else if !keep_going {
                            return Ok((false, traversed_count, Some(idx)));
                        }
                        keep_going = f(idx, v)?;
                        traversed_count += 1;
                    }
                }
            }
            Node::Link { links } => {
                let nfh = nodes_for_height(bit_width, height);
                let idx: usize = ((start_at.map_or(0, |s| s.saturating_sub(offset))) / nfh)
                    .try_into()
                    .expect("index overflow");
                for (i, link) in (idx..).zip(links[idx..].iter()) {
                    if let Some(l) = link {
                        let offs = offset + (i as u64 * nfh);
                        let (keep_going, count, next) = match l {
                            Link::Dirty(sub) => sub.for_each_while_ranged(
                                bs,
                                start_at,
                                limit.map(|l| l.checked_sub(traversed_count).unwrap()),
                                height - 1,
                                bit_width,
                                offs,
                                f,
                            )?,
                            Link::Cid { cid, cache } => {
                                let cached_node = cache.get_or_try_init(|| {
                                    bs.get_cbor::<CollapsedNode<V>>(cid)?
                                        .ok_or_else(|| Error::CidNotFound(cid.to_string()))?
                                        .expand(bit_width)
                                        .map(Box::new)
                                })?;

                                cached_node.for_each_while_ranged(
                                    bs,
                                    start_at,
                                    limit.map(|l| l.checked_sub(traversed_count).unwrap()),
                                    height - 1,
                                    bit_width,
                                    offs,
                                    f,
                                )?
                            }
                        };

                        traversed_count += count;

                        if limit.map_or(false, |l| traversed_count >= l) && next.is_some() {
                            return Ok((keep_going, traversed_count, next));
                        } else if !keep_going {
                            return Ok((false, traversed_count, next));
                        }
                    }
                }
            }
        };

        Ok((true, traversed_count, None))
    }
}

#[cfg(test)]
mod tests {
    use fvm_ipld_encoding::{from_slice, to_vec};

    use super::*;

    #[test]
    fn serialize_node_symmetric() {
        let node = Node::Leaf { vals: vec![None] };
        let nbz = to_vec(&node).unwrap();
        assert_eq!(
            from_slice::<CollapsedNode<u8>>(&nbz)
                .unwrap()
                .expand(0)
                .unwrap(),
            node
        );
    }
}
