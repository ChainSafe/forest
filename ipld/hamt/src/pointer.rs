// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::cmp::Ordering;
use std::convert::{TryFrom, TryInto};

use super::node::Node;
use super::{Error, Hash, HashAlgorithm, KeyValuePair, MAX_ARRAY_WIDTH};
use cid::Cid;
use libipld_core::ipld::Ipld;
use once_cell::unsync::OnceCell;
use serde::de::{self, DeserializeOwned};
use serde::{ser, Deserialize, Deserializer, Serialize, Serializer};

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
            (&Pointer::Values(ref a), &Pointer::Values(ref b)) => a == b,
            (&Pointer::Link { cid: ref a, .. }, &Pointer::Link { cid: ref b, .. }) => a == b,
            (&Pointer::Dirty(ref a), &Pointer::Dirty(ref b)) => a == b,
            _ => false,
        }
    }
}

/// Serialize the Pointer like an untagged enum.
impl<K, V, H> Serialize for Pointer<K, V, H>
where
    K: Serialize,
    V: Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Pointer::Values(vals) => vals.serialize(serializer),
            Pointer::Link { cid, .. } => cid.serialize(serializer),
            Pointer::Dirty(_) => Err(ser::Error::custom("Cannot serialize cached values")),
        }
    }
}

impl<K, V, H> TryFrom<Ipld> for Pointer<K, V, H>
where
    K: DeserializeOwned,
    V: DeserializeOwned,
{
    type Error = String;

    fn try_from(ipld: Ipld) -> Result<Self, Self::Error> {
        match ipld {
            ipld_list @ Ipld::List(_) => {
                let values: Vec<KeyValuePair<K, V>> =
                    Deserialize::deserialize(ipld_list).map_err(|error| error.to_string())?;
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
        Ipld::deserialize(deserializer).and_then(|ipld| ipld.try_into().map_err(de::Error::custom))
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
