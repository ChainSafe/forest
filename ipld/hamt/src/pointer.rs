// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::node::Node;
use super::{Error, Hash, HashAlgorithm, KeyValuePair, MAX_ARRAY_WIDTH};
use cid::Cid;
use lazycell::LazyCell;
use serde::de::{self, DeserializeOwned};
use serde::ser;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Pointer to index values or a link to another child node.
#[derive(Debug)]
pub(crate) enum Pointer<K, V, H> {
    Values(Vec<KeyValuePair<K, V>>),
    Link {
        cid: Cid,
        cache: LazyCell<Box<Node<K, V, H>>>,
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
            Pointer::Values(vals) => {
                #[derive(Serialize)]
                struct ValsSer<'a, A, B> {
                    #[serde(rename = "1")]
                    vals: &'a [KeyValuePair<A, B>],
                };
                ValsSer { vals }.serialize(serializer)
            }
            Pointer::Link { cid, .. } => {
                #[derive(Serialize)]
                struct LinkSer<'a> {
                    #[serde(rename = "0")]
                    cid: &'a Cid,
                };
                LinkSer { cid }.serialize(serializer)
            }
            Pointer::Dirty(_) => Err(ser::Error::custom("Cannot serialize cached values")),
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
        #[derive(Deserialize)]
        struct PointerDeser<A, B> {
            #[serde(rename = "1")]
            vals: Option<Vec<KeyValuePair<A, B>>>,

            #[serde(rename = "0")]
            cid: Option<Cid>,
        }
        let pointer_map = PointerDeser::deserialize(deserializer)?;
        match pointer_map {
            PointerDeser { vals: Some(v), .. } => Ok(Pointer::Values(v)),
            PointerDeser { cid: Some(cid), .. } => Ok(Pointer::Link {
                cid,
                cache: Default::default(),
            }),
            _ => Err(de::Error::custom("Unexpected pointer serialization")),
        }
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
                    // ! This was fixed only with the v2 actors upgrade, not used in v1
                    #[cfg(feature = "v2")]
                    {
                        use std::cmp::Ordering;

                        child_vals.sort_unstable_by(|a, b| {
                            a.key().partial_cmp(b.key()).unwrap_or(Ordering::Equal)
                        });
                    }

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
