// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::Node;
use encoding::{
    de::{self, Deserialize},
    ser::{self, Serialize},
};

/// Root of an AMT vector, can be serialized and keeps track of height and count
#[derive(PartialEq, Debug)]
pub(super) struct Root<V> {
    pub(super) height: u64,
    pub(super) count: u64,
    pub(super) node: Node<V>,
}

impl<V> Default for Root<V> {
    fn default() -> Self {
        Self {
            node: Node::default(),
            count: 0,
            height: 0,
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
        let (height, count, node) = Deserialize::deserialize(deserializer)?;
        Ok(Self {
            height,
            count,
            node,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use encoding::{from_slice, to_vec};

    #[test]
    fn serialize_symmetric() {
        let mut root = Root::default();
        root.height = 2;
        root.count = 1;
        root.node = Node::default();
        let rbz = to_vec(&root).unwrap();
        assert_eq!(from_slice::<Root<String>>(&rbz).unwrap(), root);
    }
}
