// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

use crate::{BlockStore, Error, Node, Root};
use cid::Cid;
use encoding::Cbor;

const MAX_INDEX: u64 = 1 << 48 as u64;
const WIDTH: u8 = 8;

#[derive(PartialEq, Eq, Debug)]
pub struct AMT<'a: 'db, 'db, DB>
where
    DB: BlockStore,
{
    root: Root<'a>,
    block_store: &'db DB,
}

impl<'a: 'db, 'db, DB: BlockStore> AMT<'a, 'db, DB>
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
    pub fn height(&self) -> u64 {
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
    pub fn set_node(&mut self, node: Node<'a>) -> &mut Self {
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
    pub fn set<C: Cbor>(&mut self, i: u64, val: &C) -> Result<(), Error> {
        if i >= MAX_INDEX {
            return Err(Error::OutOfRange(i));
        }

        let bz = val.marshal_cbor()?;

        while i >= nodes_for_height(WIDTH as u64, self.height() as u32) {
            // node at index exists
            if !self.node().empty() {
                // Flush non empty node
                self.root.node.flush(self.block_store, self.height())?;

                // ? why is flushed node being put in block store
                let cid = self.block_store.put(self.node())?;

                self.set_node(Node::new(vec![0x01], vec![cid]));
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

    /// flush root
    pub fn flush(&mut self) -> Result<Cid, Error> {
        let height = self.height();
        self.root.node.flush(self.block_store, height)?;
        self.block_store.put(&self.root)
    }
}

fn nodes_for_height(width: u64, height: u32) -> u64 {
    width.pow(height)
}
