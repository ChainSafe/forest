// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

use crate::{nodes_for_height, BlockStore, Error};
use cid::Cid;
use encoding::{
    de::{self, Deserialize},
    ser, Cbor,
};

type Deferred = Vec<u8>;

#[derive(PartialEq, Eq, Clone, Debug, Default)]
pub struct Node<'a> {
    pub(super) bmap: Vec<u8>,
    pub(super) links: Vec<Cid>,
    pub(super) values: Vec<Deferred>, // TODO switch to pointer if necessary

    pub(super) exp_links: Vec<Cid>,
    pub(super) exp_vals: Vec<Deferred>,
    pub(super) cache: Vec<&'a Node<'a>>,
}

impl ser::Serialize for Node<'_> {
    fn serialize<S>(&self, s: S) -> Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        (self.bmap.clone(), self.links.clone(), self.values.clone()).serialize(s)
    }
}

impl<'de> de::Deserialize<'de> for Node<'_> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        let (bmap, links, values) = Deserialize::deserialize(deserializer)?;
        Ok(Self {
            bmap,
            links,
            values,
            ..Default::default()
        })
    }
}

impl Cbor for Node<'_> {}

impl<'a> Node<'_> {
    /// Constructor
    pub fn new(bmap: Vec<u8>, links: Vec<Cid>) -> Self {
        Self {
            bmap,
            links,
            ..Default::default()
        }
    }
    pub fn flush<DB: BlockStore>(&mut self, _bs: &DB, _depth: u32) -> Result<(), Error> {
        // TODO
        todo!()
    }
    /// Check if node is empty
    pub(super) fn empty(&self) -> bool {
        self.bmap.is_empty() || self.bmap[0] == 0
    }
    /// Check if node is empty
    pub(super) fn get<DB: BlockStore>(
        &mut self,
        bs: &DB,
        height: u32,
        i: u64,
    ) -> Result<Option<Vec<u8>>, Error> {
        let subi = i / nodes_for_height(height);
        let (set, _) = self.get_bit(subi);
        if !set {
            return Ok(None);
        }
        if height == 0 {
            self.expand_values();

            let d = self.exp_vals.get(i as usize).expect("This should not fail");

            Ok(Some(d.clone()))
        } else {
            let mut subn = self.load_node(bs, subi, false)?;
            subn.get(bs, height - 1, i % nodes_for_height(height))
        }
    }
    pub(super) fn load_node<DB: BlockStore>(
        &mut self,
        _bs: &DB,
        i: u64,
        _create: bool,
    ) -> Result<Node, Error> {
        if self.cache.is_empty() {
            self.expand_links();
        } else if let Some(v) = self.cache.get(i as usize) {
            return Ok(Node::clone(v));
        }

        todo!()
    }
    fn expand_values(&mut self) {
        todo!()
    }
    fn expand_links(&mut self) {
        todo!()
    }
    fn get_bit(&self, i: u64) -> (bool, u64) {
        if i > 7 {
            panic!("can't deal with wider than 7 arrays");
        }
        // TODO
        (true, 0)
    }
    /// set value in node
    pub(super) fn set<DB: BlockStore>(
        &mut self,
        _bs: &DB,
        _height: u32,
        _i: u64,
        _val: &[u8],
    ) -> Result<bool, Error> {
        // TODO
        todo!()
    }
}
