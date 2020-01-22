// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

use crate::{BlockStore, Node};

#[derive(PartialEq, Eq, Debug)]
pub struct Root<'a, DB>
where
    DB: BlockStore,
{
    _height: u64,
    _count: u64,
    _node: Node<'a>,
    block_store: DB,
}

impl<'a, DB> Root<'a, DB>
where
    DB: BlockStore,
{
    pub fn new(block_store: DB) -> Self {
        Self {
            _height: 0,
            _count: 0,
            _node: Node::default(),
            block_store,
        }
    }
}
