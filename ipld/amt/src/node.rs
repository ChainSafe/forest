// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

use crate::{BlockStore, Error};
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

    pub(super) _exp_links: Vec<Cid>,
    pub(super) _exp_vals: Vec<Deferred>,
    pub(super) _cache: Vec<&'a Node<'a>>,
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
    pub fn flush<DB: BlockStore>(&mut self, _bs: &DB, _depth: u64) -> Result<(), Error> {
        // TODO
        todo!()
    }
    /// Check if node is empty
    pub(super) fn empty(&self) -> bool {
        self.bmap.is_empty() || self.bmap[0] == 0
    }
    /// set value in node
    pub(super) fn set<DB: BlockStore>(
        &mut self,
        _bs: &DB,
        _height: u64,
        _i: u64,
        _val: &[u8],
    ) -> Result<bool, Error> {
        // TODO
        todo!()
    }
}
