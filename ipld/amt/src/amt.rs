// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::{
    node::Link, nodes_for_height, BitMap, Error, Node, Root, MAX_HEIGHT, MAX_INDEX, WIDTH,
};
use cid::{Cid, Code::Blake2b256};
use encoding::{de::DeserializeOwned, ser::Serialize};
use ipld_blockstore::BlockStore;
use std::error::Error as StdError;

use super::ValueMut;

/// Array Mapped Trie allows for the insertion and persistence of data, serializable to a CID.
///
/// Amt is not threadsafe and can't be shared between threads.
///
/// Usage:
/// ```
/// use ipld_amt::Amt;
///
/// let db = db::MemoryDB::default();
/// let mut amt = Amt::new(&db);
///
/// // Insert or remove any serializable values
/// amt.set(2, "foo".to_owned()).unwrap();
/// amt.set(1, "bar".to_owned()).unwrap();
/// amt.delete(2).unwrap();
/// assert_eq!(amt.count(), 1);
/// let bar: &String = amt.get(1).unwrap().unwrap();
///
/// // Generate cid by calling flush to remove cache
/// let cid = amt.flush().unwrap();
/// ```
#[derive(Debug)]
pub struct Amt<'db, V, BS> {
    root: Root<V>,
    block_store: &'db BS,
}

impl<'a, V: PartialEq, BS: BlockStore> PartialEq for Amt<'a, V, BS> {
    fn eq(&self, other: &Self) -> bool {
        self.root == other.root
    }
}

impl<'db, V, BS> Amt<'db, V, BS>
where
    V: DeserializeOwned + Serialize,
    BS: BlockStore,
{
    /// Constructor for Root AMT node
    pub fn new(block_store: &'db BS) -> Self {
        Self {
            root: Root::default(),
            block_store,
        }
    }

    /// Constructs an AMT with a blockstore and a Cid of the root of the AMT
    pub fn load(cid: &Cid, block_store: &'db BS) -> Result<Self, Error> {
        // Load root bytes from database
        let root: Root<V> = block_store
            .get(cid)?
            .ok_or_else(|| Error::CidNotFound(cid.to_string()))?;

        // Sanity check, this should never be possible.
        if root.height > MAX_HEIGHT {
            return Err(Error::MaxHeight(root.height, MAX_HEIGHT));
        }

        Ok(Self { root, block_store })
    }

    // Getter for height
    pub fn height(&self) -> u64 {
        self.root.height
    }

    // Getter for count
    pub fn count(&self) -> u64 {
        self.root.count
    }

    /// Generates an AMT with block store and array of cbor marshallable objects and returns Cid
    pub fn new_from_slice(block_store: &'db BS, vals: &[V]) -> Result<Cid, Error>
    where
        V: Clone,
    {
        let mut t = Self::new(block_store);

        t.batch_set(vals)?;

        t.flush()
    }

    /// Get value at index of AMT
    pub fn get(&self, i: u64) -> Result<Option<&V>, Error> {
        if i > MAX_INDEX {
            return Err(Error::OutOfRange(i));
        }

        if i >= nodes_for_height(self.height() + 1) {
            return Ok(None);
        }

        self.root.node.get(self.block_store, self.height(), i)
    }

    /// Set value at index
    pub fn set(&mut self, i: u64, val: V) -> Result<(), Error> {
        if i > MAX_INDEX {
            return Err(Error::OutOfRange(i));
        }

        while i >= nodes_for_height(self.height() + 1) {
            // node at index exists
            if !self.root.node.empty() {
                // Parent node for expansion
                let mut new_links: [Option<Link<V>>; WIDTH] = Default::default();

                #[cfg(feature = "go-interop")]
                {
                    // Save and get cid to be able to link from higher level node
                    self.root.node.flush(self.block_store)?;

                    // Get cid from storing root node
                    let cid = self.block_store.put(&self.root.node, Blake2b256)?;

                    // Set link to child node being expanded
                    new_links[0] = Some(Link::from(cid));
                }
                #[cfg(not(feature = "go-interop"))]
                {
                    // Take root node to be moved down
                    let node = std::mem::take(&mut self.root.node);

                    // Set link to child node being expanded
                    new_links[0] = Some(Link::Dirty(Box::new(node)));
                }

                self.root.node = Node::Link {
                    bmap: BitMap::new(0x01),
                    links: new_links,
                };
            } else {
                // If first expansion is before a value inserted, convert base node to Link
                self.root.node = Node::Link {
                    bmap: Default::default(),
                    links: Default::default(),
                };
            }
            // Incrememnt height after each iteration
            self.root.height += 1;
        }

        if self
            .root
            .node
            .set(self.block_store, self.height(), i, val)?
        {
            self.root.count += 1;
        }

        Ok(())
    }

    /// Batch set (naive for now)
    // TODO Implement more efficient batch set to not have to traverse tree and keep cache for each
    pub fn batch_set(&mut self, vals: &[V]) -> Result<(), Error>
    where
        V: Clone,
    {
        for (i, val) in vals.iter().enumerate() {
            self.set(i as u64, val.clone())?;
        }

        Ok(())
    }

    /// Delete item from AMT at index
    pub fn delete(&mut self, i: u64) -> Result<bool, Error> {
        if i > MAX_INDEX {
            return Err(Error::OutOfRange(i));
        }

        if i >= nodes_for_height(self.height() + 1) {
            // Index was out of range of current AMT
            return Ok(false);
        }

        // Delete node from AMT
        if !self.root.node.delete(self.block_store, self.height(), i)? {
            return Ok(false);
        }

        self.root.count -= 1;

        // Handle height changes from delete
        while *self.root.node.bitmap() == 0x01 && self.height() > 0 {
            let sub_node: Node<V> = match &mut self.root.node {
                Node::Link { links, .. } => match &mut links[0] {
                    Some(Link::Dirty(node)) => *std::mem::take(node),
                    Some(Link::Cid { cid, cache }) => {
                        let cache_node = std::mem::take(cache);
                        if let Some(sn) = cache_node.into_inner() {
                            *sn
                        } else {
                            // Only retrieve sub node if not found in cache
                            self.block_store
                                .get(&cid)?
                                .ok_or_else(|| Error::RootNotFound)?
                        }
                    }
                    _ => unreachable!("Link index should match bitmap"),
                },
                Node::Leaf { .. } => unreachable!("Non zero height cannot be a leaf node"),
            };

            self.root.node = sub_node;
            self.root.height -= 1;
        }

        Ok(true)
    }

    /// Deletes multiple items from AMT
    pub fn batch_delete(&mut self, iter: impl IntoIterator<Item = u64>) -> Result<(), Error> {
        // TODO: optimize this
        for i in iter {
            self.delete(i)?;
        }
        Ok(())
    }

    /// flush root and return Cid used as key in block store
    pub fn flush(&mut self) -> Result<Cid, Error> {
        self.root.node.flush(self.block_store)?;
        Ok(self.block_store.put(&self.root, Blake2b256)?)
    }

    /// Iterates over each value in the Amt and runs a function on the values.
    ///
    /// The index in the amt is a `u64` and the value is the generic parameter `V` as defined
    /// in the Amt.
    ///
    /// # Examples
    ///
    /// ```
    /// use ipld_amt::Amt;
    ///
    /// let store = db::MemoryDB::default();
    ///
    /// let mut map: Amt<String, _> = Amt::new(&store);
    /// map.set(1, "One".to_owned()).unwrap();
    /// map.set(4, "Four".to_owned()).unwrap();
    ///
    /// let mut values: Vec<(u64, String)> = Vec::new();
    /// map.for_each(|i, v| {
    ///    values.push((i, v.clone()));
    ///    Ok(())
    /// }).unwrap();
    /// assert_eq!(&values, &[(1, "One".to_owned()), (4, "Four".to_owned())]);
    /// ```
    #[inline]
    pub fn for_each<F>(&self, mut f: F) -> Result<(), Box<dyn StdError>>
    where
        V: DeserializeOwned,
        F: FnMut(u64, &V) -> Result<(), Box<dyn StdError>>,
    {
        self.for_each_while(|i, x| {
            f(i, x)?;
            Ok(true)
        })
    }

    /// Iterates over each value in the Amt and runs a function on the values, for as long as that
    /// function keeps returning `true`.
    pub fn for_each_while<F>(&self, mut f: F) -> Result<(), Box<dyn StdError>>
    where
        V: DeserializeOwned,
        F: FnMut(u64, &V) -> Result<bool, Box<dyn StdError>>,
    {
        self.root
            .node
            .for_each_while(self.block_store, self.height(), 0, &mut f)
            .map(|_| ())
    }

    /// Iterates over each value in the Amt and runs a function on the values that allows modifying
    /// each value.
    pub fn for_each_mut<F>(&mut self, mut f: F) -> Result<(), Box<dyn StdError>>
    where
        V: DeserializeOwned,
        F: FnMut(u64, &mut ValueMut<'_, V>) -> Result<(), Box<dyn StdError>>,
    {
        self.for_each_while_mut(|i, x| {
            f(i, x)?;
            Ok(true)
        })
    }

    /// Iterates over each value in the Amt and runs a function on the values that allows modifying
    /// each value, for as long as that function keeps returning `true`.
    pub fn for_each_while_mut<F>(&mut self, mut f: F) -> Result<(), Box<dyn StdError>>
    where
        V: DeserializeOwned,
        F: FnMut(u64, &mut ValueMut<'_, V>) -> Result<bool, Box<dyn StdError>>,
    {
        self.root
            .node
            .for_each_while_mut(self.block_store, self.height(), 0, &mut f)
            .map(|_| ())
    }
}
