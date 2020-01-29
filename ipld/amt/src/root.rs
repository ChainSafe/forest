// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

use crate::Node;
use encoding::{
    de::{self, Deserialize},
    ser,
};

#[derive(PartialEq, Eq, Debug, Default)]
pub struct Root {
    pub(super) height: u32,
    pub(super) count: u64,
    pub(super) node: Node,
}

impl ser::Serialize for Root {
    fn serialize<S>(&self, s: S) -> Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        (self.height, self.count, self.node.clone()).serialize(s)
    }
}

impl<'de> de::Deserialize<'de> for Root {
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
        root.node = Node::new(10, Default::default());
        let rbz = to_vec(&root).unwrap();
        assert_eq!(from_slice::<Root>(&rbz).unwrap(), root);
    }
}
