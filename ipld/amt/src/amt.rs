// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

use crate::{
    node::{LinkNode, Values},
    nodes_for_height, BlockStore, Error, Node, Root, MAX_INDEX, WIDTH,
};
use cid::Cid;
use encoding::{ser::Serialize, to_vec};

#[derive(PartialEq, Eq, Debug)]
pub struct AMT<'db, DB>
where
    DB: BlockStore,
{
    root: Root,
    block_store: &'db DB,
}

impl<'db, DB: BlockStore> AMT<'db, DB>
where
    DB: BlockStore,
{
    /// Constructor for Root AMT node
    pub fn new(block_store: &'db DB) -> Self {
        Self {
            root: Root::default(),
            block_store,
        }
    }

    // Getter for height
    pub fn height(&self) -> u32 {
        self.root.height
    }

    // Getter for count
    pub fn count(&self) -> u64 {
        self.root.count
    }

    // Getter for node
    pub fn node(&self) -> &Node {
        &self.root.node
    }
    /// Sets root node
    pub fn set_node(&mut self, node: Node) -> &mut Self {
        self.root.node = node;
        self
    }

    /// Constructor from array of cbor marshallable objects and return Cid
    // ? Should this instead be a constructor
    pub fn new_from_array(block_store: &'db DB, vals: Vec<&[u8]>) -> Result<Cid, Error> {
        let mut t = Self::new(block_store);

        t.batch_set(vals)?;

        t.flush()
    }
    /// Set value at index
    pub fn set<S>(&mut self, i: u64, val: &S) -> Result<(), Error>
    where
        S: Serialize,
    {
        if i >= MAX_INDEX {
            return Err(Error::OutOfRange(i));
        }

        let bz = to_vec(val)?;

        while i >= nodes_for_height(self.height() + 1 as u32) {
            // node at index exists
            if !self.node().empty() {
                // Get cid to be able to link from higher level shard
                let cid = self.block_store.put(self.node())?;

                // Set links node with first index as cid
                let mut new_links: [LinkNode; WIDTH] = Default::default();
                new_links[0] = LinkNode::Cid(cid);

                self.set_node(Node::new(0x01, Values::Links(new_links)));
            }
            // Incrememnt height after each iteration
            self.root.height += 1;
        }

        if self
            .root
            .node
            .set(self.block_store, self.height(), i, &bz)?
        {
            self.root.count += 1;
        }

        Ok(())
    }

    /// Batch set (naive for now)
    pub fn batch_set(&mut self, vals: Vec<&[u8]>) -> Result<(), Error> {
        for (i, val) in vals.iter().enumerate() {
            self.set(i as u64, val)?;
        }
        Ok(())
    }

    pub fn get(&mut self, i: u64) -> Result<Option<Vec<u8>>, Error> {
        if i >= MAX_INDEX {
            return Err(Error::OutOfRange(i));
        }

        if i >= nodes_for_height(self.height() + 1) {
            return Ok(None);
        }

        self.root.node.get(self.block_store, self.height(), i)
    }

    /// flush root
    pub fn flush(&mut self) -> Result<Cid, Error> {
        let height = self.height();
        self.root.node.flush(self.block_store, height)?;
        self.block_store.put(&self.root)
    }
}
