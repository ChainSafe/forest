// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::{init_sized_vec, node::CollapsedNode, Node, DEFAULT_BIT_WIDTH};
use serde::{
    de::{self, Deserialize},
    ser::{self, Serialize},
};

/// Root of an AMT vector, can be serialized and keeps track of height and count
#[derive(PartialEq, Debug, Clone)]
pub(super) struct Root<V> {
    pub bit_width: usize,
    pub height: usize,
    pub count: usize,
    pub node: Node<V>,
}

impl<V> Root<V> {
    pub(super) fn new() -> Self {
        Self {
            bit_width: DEFAULT_BIT_WIDTH,
            count: 0,
            height: 0,
            node: Node::Leaf {
                vals: init_sized_vec(DEFAULT_BIT_WIDTH),
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
        // This serialization is here for legacy reasons only. For new AMTs it should not be used.
        // (&self.bit_width, &self.height, &self.count, &self.node).serialize(s)
        (&self.height, &self.count, &self.node).serialize(s)
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
        let (height, count, node): (_, _, CollapsedNode<V>) =
            Deserialize::deserialize(deserializer)?;
        Ok(Self {
            bit_width: DEFAULT_BIT_WIDTH,
            height,
            count,
            node: node.expand(DEFAULT_BIT_WIDTH).map_err(de::Error::custom)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quickcheck_macros::quickcheck;
    use serde_json;

    impl<V: std::clone::Clone + 'static + serde::Serialize + for<'de> serde::Deserialize<'de>> quickcheck::Arbitrary for Root<V> {
        fn arbitrary(g: &mut quickcheck::Gen) -> Self {
            let node = Node::empty();
            Root {
                bit_width: usize::arbitrary(g),
                height: usize::arbitrary(g),
                count: usize::arbitrary(g),
                node: node,
            }
        }
    }

    #[quickcheck]
    fn root_roundtrip<V: std::cmp::PartialEq + std::fmt::Debug>(root: Root<V>) {
        let serialized: String = forest_test_utils::to_string_with!(&root, json::serialize);
        let parsed = forest_test_utils::from_str_with!(&serialized, json::deserialize);
        assert_eq!(root, parsed);
    }
}
