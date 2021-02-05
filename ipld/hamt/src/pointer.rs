// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::node::Node;
use super::{Error, Hash, HashAlgorithm, KeyValuePair, MAX_ARRAY_WIDTH};
use cid::Cid;
use once_cell::unsync::OnceCell;
use serde::de::DeserializeOwned;
use serde::ser;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::cmp::Ordering;
use std::convert::TryFrom;

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

#[derive(Serialize)]
#[serde(untagged)]
enum PointerSer<'a, K, V> {
    Vals(&'a [KeyValuePair<K, V>]),
    Link(&'a Cid),
}

impl<'a, K, V, H> TryFrom<&'a Pointer<K, V, H>> for PointerSer<'a, K, V> {
    type Error = &'static str;

    fn try_from(pointer: &'a Pointer<K, V, H>) -> Result<Self, Self::Error> {
        match pointer {
            Pointer::Values(vals) => Ok(PointerSer::Vals(vals.as_ref())),
            Pointer::Link { cid, .. } => Ok(PointerSer::Link(cid)),
            Pointer::Dirty(_) => Err("Cannot serialize cached values"),
        }
    }
}

impl<K, V, H> Serialize for Pointer<K, V, H>
where
    K: Serialize,
    V: Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        PointerSer::try_from(self)
            .map_err(ser::Error::custom)?
            .serialize(serializer)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum PointerDe<K, V> {
    Vals(Vec<KeyValuePair<K, V>>),
    Link(Cid),
}

impl<K, V, H> From<PointerDe<K, V>> for Pointer<K, V, H> {
    fn from(pointer: PointerDe<K, V>) -> Self {
        match pointer {
            PointerDe::Link(cid) => Pointer::Link {
                cid,
                cache: Default::default(),
            },
            PointerDe::Vals(vals) => Pointer::Values(vals),
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
        let pointer_de: PointerDe<K, V> = Deserialize::deserialize(deserializer)?;
        Ok(Pointer::from(pointer_de))
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
