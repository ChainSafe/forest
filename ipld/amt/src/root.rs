// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

use crate::Node;
use encoding::{
    de::{self, Deserialize, DeserializeOwned},
    ser::{self, Serialize},
};

/// Root of an AMT vector, can be serialized and keeps track of height and count
#[derive(PartialEq, Debug, Default)]
pub(super) struct Root<V>
where
    V: Clone + Serialize,
{
    pub(super) height: u32,
    pub(super) count: u64,
    pub(super) node: Node<V>,
}

impl<V> ser::Serialize for Root<V>
where
    V: Clone + PartialEq + Serialize,
    Node<V>: Clone,
{
    fn serialize<S>(&self, s: S) -> Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        (self.height, self.count, self.node.clone()).serialize(s)
    }
}

impl<'de, V> de::Deserialize<'de> for Root<V>
where
    V: Clone + PartialEq + Serialize + DeserializeOwned,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
        V: DeserializeOwned,
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
