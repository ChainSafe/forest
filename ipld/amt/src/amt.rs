// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

use crate::{
    node::{LinkNode, Values},
    nodes_for_height, BlockStore, Error, Node, Root, MAX_INDEX, WIDTH,
};
use cid::Cid;
use encoding::{from_slice, ser::Serialize, to_vec};

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

    /// Constructs an AMT with a blockstore and a Cid of the root of the AMT
    pub fn load(block_store: &'db DB, cid: &Cid) -> Result<Self, Error> {
        // Load root bytes from database
        let root_bz = block_store
            .get(cid)?
            .ok_or_else(|| Error::Db("Root not found in database".to_owned()))?;
        let root: Root = from_slice(&root_bz)?;

        Ok(Self { root, block_store })
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

    /// Getter for root of AMT
    pub fn root(&self) -> &Root {
        &self.root
    }

    /// Sets root node
    pub fn set_node(&mut self, node: Node) {
        self.root.node = node;
    }

    /// Constructor from array of cbor marshallable objects and return Cid
    // ? Should this instead be a constructor
    pub fn new_from_slice<S>(block_store: &'db DB, vals: &[&S]) -> Result<Cid, Error>
    where
        S: Serialize,
    {
        let mut t = Self::new(block_store);

        t.batch_set(vals)?;

        t.flush()
    }

    /// Get bytes at index of AMT
    pub fn get(&mut self, i: u64) -> Result<Option<Vec<u8>>, Error> {
        if i >= MAX_INDEX {
            return Err(Error::OutOfRange(i));
        }

        if i >= nodes_for_height(self.height() + 1) {
            return Ok(None);
        }

        self.root.node.get(self.block_store, self.height(), i)
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
                // Save and get cid to be able to link from higher level node
                self.root.node.flush(self.block_store)?;

                // Get cid from storing root node
                let cid = self.block_store.put(&self.root.node)?;

                // Set links node with first index as cid
                let mut new_links: [LinkNode; WIDTH] = Default::default();
                new_links[0] = LinkNode::Cid(cid);

                self.set_node(Node::new(0x01, Values::Links(new_links)));
            } else {
                // If first expansion is before a value inserted, convert base node to Link
                self.set_node(Node::new(0x00, Values::Links(Default::default())));
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
    pub fn batch_set<S>(&mut self, vals: &[&S]) -> Result<(), Error>
    where
        S: Serialize,
    {
        for (i, val) in vals.iter().enumerate() {
            self.set(i as u64, val)?;
        }

        Ok(())
    }

    /// Delete item from AMT at index
    pub fn delete(&mut self, i: u64) -> Result<bool, Error> {
        if i >= MAX_INDEX {
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
        while self.root.node.bmap == 0x01 && self.height() > 0 {
            let sub_node: Node = match &self.root.node.vals {
                Values::Links(l) => match l.get(0) {
                    Some(LinkNode::Cached(node)) => *node.clone(),
                    Some(LinkNode::Cid(cid)) => {
                        let res: Vec<u8> = self.block_store.get(cid)?.ok_or_else(|| {
                            Error::Cid("Cid did not match any in database".to_owned())
                        })?;

                        from_slice(&res)?
                    }
                    _ => return Err(Error::Custom("Link index should match bitmap".to_owned())),
                },
                Values::Leaf(_) => unreachable!("Non zero height cannot be a leaf node"),
            };

            self.set_node(sub_node);
            self.root.height -= 1;
        }

        Ok(true)
    }

    /// flush root and return Cid used as key in block store
    pub fn flush(&mut self) -> Result<Cid, Error> {
        self.root.node.flush(self.block_store)?;
        self.block_store.put(&self.root)
    }
}
