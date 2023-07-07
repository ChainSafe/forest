// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
// Copyright 2021-2023 Protocol Labs
// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::marker::PhantomData;

use serde::de::{self, Deserialize};
use serde::ser::{self, Serialize};

use super::node::CollapsedNode;
use super::{init_sized_vec, Node, DEFAULT_BIT_WIDTH};

pub(super) mod version {
    #[derive(PartialEq, Eq, Debug)]
    pub struct V0;
    #[derive(PartialEq, Eq, Debug)]
    pub struct V3;

    pub trait Version {
        const NUMBER: usize;
    }

    impl Version for V0 {
        const NUMBER: usize = 0;
    }

    impl Version for V3 {
        const NUMBER: usize = 3;
    }
}

#[derive(PartialEq, Debug)]
pub(super) struct RootImpl<V, Ver> {
    pub bit_width: u32,
    pub height: u32,
    pub count: u64,
    pub node: Node<V>,
    ver: PhantomData<Ver>,
}

impl<V, Ver> RootImpl<V, Ver> {
    pub(super) fn new_with_bit_width(bit_width: u32) -> Self {
        Self {
            bit_width,
            count: 0,
            height: 0,
            node: Node::Leaf {
                vals: init_sized_vec(bit_width),
            },
            ver: PhantomData,
        }
    }
}

impl<V, Ver> Serialize for RootImpl<V, Ver>
where
    V: Serialize,
    Ver: self::version::Version,
{
    fn serialize<S>(&self, s: S) -> Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        match Ver::NUMBER {
            // legacy amt v0 doesn't serialize bit_width as DEFAULT_BIT_WIDTH is used.
            0 => (&self.height, &self.count, &self.node).serialize(s),
            3 => (&self.bit_width, &self.height, &self.count, &self.node).serialize(s),
            _ => unreachable!(),
        }
    }
}

impl<'de, V, Ver> Deserialize<'de> for RootImpl<V, Ver>
where
    V: Deserialize<'de>,
    Ver: self::version::Version,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        match Ver::NUMBER {
            3 => {
                let (bit_width, height, count, node): (_, _, _, CollapsedNode<V>) =
                    Deserialize::deserialize(deserializer)?;
                Ok(Self {
                    bit_width,
                    height,
                    count,
                    node: node.expand(bit_width).map_err(de::Error::custom)?,
                    ver: PhantomData,
                })
            }
            // legacy amt v0
            0 => {
                let (height, count, node): (_, _, CollapsedNode<V>) =
                    Deserialize::deserialize(deserializer)?;
                Ok(Self {
                    bit_width: DEFAULT_BIT_WIDTH,
                    height,
                    count,
                    node: node.expand(DEFAULT_BIT_WIDTH).map_err(de::Error::custom)?,
                    ver: PhantomData,
                })
            }
            _ => unreachable!(),
        }
    }
}

#[cfg(test)]
mod tests {
    use fvm_ipld_encoding::{from_slice, to_vec};

    use super::*;

    /// Root of an AMT vector, can be serialized and keeps track of height and count
    type Root<V> = RootImpl<V, self::version::V3>;
    /// Legacy `AMT v0`, used to read block headers.
    type Rootv0<V> = RootImpl<V, self::version::V0>;

    impl<V> RootImpl<V, self::version::V0> {
        pub(super) fn new() -> Rootv0<V> {
            Self {
                bit_width: DEFAULT_BIT_WIDTH,
                count: 0,
                height: 0,
                node: Node::Leaf {
                    vals: init_sized_vec(DEFAULT_BIT_WIDTH),
                },
                ver: PhantomData,
            }
        }
    }

    #[test]
    fn serialize_symmetric() {
        let mut root = Root::new_with_bit_width(0);
        root.height = 2;
        root.count = 1;
        root.node = Node::Leaf { vals: vec![None] };
        let rbz = to_vec(&root).unwrap();
        assert_eq!(from_slice::<Root<String>>(&rbz).unwrap(), root);
    }

    #[test]
    fn serialize_deserialize_legacy_amt() {
        let mut root: Rootv0<_> = Rootv0::new();
        root.height = 2;
        root.count = 1;
        root.node = Node::Leaf {
            vals: vec![None; 8],
        };
        let rbz = to_vec(&root).unwrap();
        assert_eq!(from_slice::<Rootv0<String>>(&rbz).unwrap(), root);
    }
}
