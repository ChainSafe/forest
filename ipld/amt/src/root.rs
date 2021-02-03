// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::{init_sized_vec, node::CollapsedNode, Node};
use serde::{
    de::{self, Deserialize},
    ser::{self, Serialize},
};

/// Root of an AMT vector, can be serialized and keeps track of height and count
#[derive(PartialEq, Debug)]
pub(super) struct Root<V> {
    pub bit_width: usize,
    pub height: usize,
    pub count: usize,
    pub node: Node<V>,
}

impl<V> Root<V> {
    pub(super) fn new(bit_width: usize) -> Self {
        Self {
            bit_width,
            count: 0,
            height: 0,
            node: Node::Leaf {
                vals: init_sized_vec(bit_width),
            },
        }
    }
}

impl<V> Serialize for Root<V>
where
    V: Serialize,
{
    fn serialize<S>(&self, s: S) -> Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        (&self.bit_width, &self.height, &self.count, &self.node).serialize(s)
    }
}

impl<'de, V> Deserialize<'de> for Root<V>
where
    V: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        let (bit_width, height, count, node): (_, _, _, CollapsedNode<V>) =
            Deserialize::deserialize(deserializer)?;
        Ok(Self {
            bit_width,
            height,
            count,
            node: node.expand(bit_width).map_err(de::Error::custom)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use encoding::{from_slice, to_vec};

    #[test]
    fn serialize_symmetric() {
        let mut root = Root::new(0);
        root.height = 2;
        root.count = 1;
        root.node = Node::Leaf { vals: vec![None] };
        let rbz = to_vec(&root).unwrap();
        assert_eq!(from_slice::<Root<String>>(&rbz).unwrap(), root);
    }
}
