// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::node::Node;
use super::KeyValuePair;
use cid::Cid;
use libipld::Ipld;
use once_cell::unsync::OnceCell;
use serde::de::{self, DeserializeOwned};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Pointer to index values or a link to another child node.
#[derive(Debug)]
pub(crate) enum Pointer<K, V, H> {
    Values(Vec<KeyValuePair<K, V>>),
    Link {
        cid: Cid,
        cache: OnceCell<Box<Node<K, V, H>>>,
    },
    #[allow(dead_code)]
    Dirty(Box<Node<K, V, H>>),
}

impl<K: PartialEq, V: PartialEq, H> PartialEq for Pointer<K, V, H> {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Pointer::Values(ref a), Pointer::Values(ref b)) => a == b,
            (Pointer::Link { cid: ref a, .. }, Pointer::Link { cid: ref b, .. }) => a == b,
            (Pointer::Dirty(ref a), Pointer::Dirty(ref b)) => a == b,
            _ => false,
        }
    }
}

impl<K, V, H> Serialize for Pointer<K, V, H>
where
    K: Serialize,
    V: Serialize,
{
    fn serialize<S>(&self, _serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        todo!("This code is wrong. It should match the deserializer.")
    }
}

impl<'de, K, V, H> Deserialize<'de> for Pointer<K, V, H>
where
    K: DeserializeOwned,
    V: DeserializeOwned,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let ipld = Ipld::deserialize(deserializer)?;
        let (_key, value) = match ipld {
            Ipld::Map(map) => map
                .into_iter()
                .next()
                .ok_or("Expected at least one element".to_string()),
            other => Err(format!("Expected `Ipld::Map`, got {:#?}", other)),
        }
        .map_err(de::Error::custom)?;
        match value {
            ipld_list @ Ipld::List(_) => {
                let values: Vec<KeyValuePair<K, V>> =
                    Deserialize::deserialize(ipld_list).map_err(de::Error::custom)?;
                Ok(Self::Values(values))
            }
            Ipld::Link(cid) => Ok(Self::Link {
                cid,
                cache: Default::default(),
            }),
            other => Err(format!(
                "Expected `Ipld::List` or `Ipld::Link`, got {:#?}",
                other
            )),
        }
        .map_err(de::Error::custom)
    }
}

impl<K, V, H> Default for Pointer<K, V, H> {
    fn default() -> Self {
        Pointer::Values(Vec::new())
    }
}
