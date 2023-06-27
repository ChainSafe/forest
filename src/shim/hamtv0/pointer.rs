// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::node::Node;
use super::{Error, Hash, HashAlgorithm, KeyValuePair, MAX_ARRAY_WIDTH};
use cid::Cid;
use libipld::Ipld;
use once_cell::unsync::OnceCell;
use serde::de::{self, DeserializeOwned};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::cmp::Ordering;

#[test]
fn pointer_round_trip() {
    type Test = Pointer<u8, u8, sha2::Sha256>;

    let empty_values: Test = Pointer::Values(vec![]);
    let values: Test = Pointer::Values(vec![KeyValuePair(1, 1)]);
    let link: Test = Pointer::Link {
        cid: Cid::default(),
        cache: OnceCell::new(),
    };

    for case in [empty_values, values, link] {
        println!("{case:?}");
        let serialized = fvm_ipld_encoding::to_vec(&case).unwrap();
        let deserialized = fvm_ipld_encoding::from_slice::<Test>(&serialized).unwrap();
        assert_eq!(deserialized, case);
    }
}

#[test]
fn link_round_trip() {
    type Test = Pointer<u8, u8, sha2::Sha256>;
    let link: Test = Pointer::Link {
        cid: Cid::default(),
        cache: OnceCell::new(),
    };

    let serialized = fvm_ipld_encoding::to_vec(&link).unwrap();
    let deserialized = fvm_ipld_encoding::from_slice::<Test>(&serialized).unwrap();
    assert_eq!(deserialized, link);
}

#[test]
fn cid_round_trip() {
    let cid = Cid::default();
    let serialized = fvm_ipld_encoding::to_vec(&cid).unwrap();
    let deserialized = fvm_ipld_encoding::from_slice::<Cid>(&serialized).unwrap();
    assert_eq!(deserialized, cid);
}

/// Pointer to index values or a link to another child node.
#[derive(Debug)]
pub(crate) enum Pointer<K, V, H> {
    Values(Vec<KeyValuePair<K, V>>),
    Link {
        cid: Cid,
        cache: OnceCell<Box<Node<K, V, H>>>,
    },
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

impl<K, V, H> Pointer<K, V, H>
where
    K: Serialize + DeserializeOwned + Hash + PartialOrd,
    V: Serialize + DeserializeOwned,
    H: HashAlgorithm,
{
    pub(crate) fn from_key_value(key: K, value: V) -> Self {
        Pointer::Values(vec![KeyValuePair::new(key, value)])
    }

    /// Internal method to cleanup children, to ensure consistent tree representation
    /// after deletes.
    pub(crate) fn clean(&mut self) -> Result<(), Error> {
        // todo!()
        match self {
            Pointer::Dirty(n) => match n.pointers.len() {
                0 => Err(Error::ZeroPointers),
                1 => {
                    // Node has only one pointer, swap with parent node
                    if let Pointer::Values(vals) = &mut n.pointers[0] {
                        // Take child values, to ensure canonical ordering
                        let values = std::mem::take(vals);

                        // move parent node up
                        *self = Pointer::Values(values)
                    }
                    Ok(())
                }
                2..=MAX_ARRAY_WIDTH => {
                    // If more child values than max width, nothing to change.
                    let mut children_len = 0;
                    for c in n.pointers.iter() {
                        if let Pointer::Values(vals) = c {
                            children_len += vals.len();
                        } else {
                            return Ok(());
                        }
                    }
                    if children_len > MAX_ARRAY_WIDTH {
                        return Ok(());
                    }

                    // Collect values from child nodes to collapse.
                    #[allow(unused_mut)]
                    let mut child_vals: Vec<KeyValuePair<K, V>> = n
                        .pointers
                        .iter_mut()
                        .filter_map(|p| {
                            if let Pointer::Values(kvs) = p {
                                Some(std::mem::take(kvs))
                            } else {
                                None
                            }
                        })
                        .flatten()
                        .collect();

                    // Sorting by key, values are inserted based on the ordering of the key itself,
                    // so when collapsed, it needs to be ensured that this order is equal.
                    child_vals.sort_unstable_by(|a, b| {
                        a.key().partial_cmp(b.key()).unwrap_or(Ordering::Equal)
                    });

                    // Replace link node with child values
                    *self = Pointer::Values(child_vals);
                    Ok(())
                }
                _ => Ok(()),
            },
            _ => unreachable!("clean is only called on dirty pointer"),
        }
    }
}
