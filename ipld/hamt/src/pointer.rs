// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::node::Node;
use super::{Error, KeyValuePair};
use cid::Cid;
use serde::de::{self, DeserializeOwned};
use serde::ser;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) enum Pointer<K, V> {
    Values(Vec<KeyValuePair<K, V>>),
    Link(Cid),
    Cache(Box<Node<K, V>>),
}

impl<K, V> Serialize for Pointer<K, V>
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
            Pointer::Link(cid) => {
                #[derive(Serialize)]
                struct LinkSer<'a> {
                    #[serde(rename = "0")]
                    cid: &'a Cid,
                };
                LinkSer { cid }.serialize(serializer)
            }
            Pointer::Cache(_) => Err(ser::Error::custom("Cannot serialize cached values")),
        }
    }
}

impl<'de, K, V> Deserialize<'de> for Pointer<K, V>
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
            PointerDeser { cid: Some(cid), .. } => Ok(Pointer::Link(cid)),
            _ => Err(de::Error::custom("Unexpected pointer serialization")),
        }
    }
}

impl<K, V> Default for Pointer<K, V> {
    fn default() -> Self {
        Pointer::Values(Vec::new())
    }
}

impl<K, V> Pointer<K, V>
where
    K: Serialize + DeserializeOwned,
    V: Serialize + DeserializeOwned,
{
    pub fn from_key_value(key: K, value: V) -> Self {
        Pointer::Values(vec![KeyValuePair::new(key, value)])
    }

    /// Internal method to cleanup children, to ensure consistent tree representation
    /// after deletes.
    pub fn _clean(&mut self) -> Result<(), Error> {
        //     let len = if let Pointer::Link { cache, .. } = self {
        //         assert!(cache.filled());
        //         cache.borrow().unwrap().pointers.len()
        //     } else {
        //         panic!("Should be shard node here");
        //     };
        //     if len == 0 {
        //         return Err(Error::Custom("Invalid HAMT"));
        //     }

        //     replace_with(
        //         self,
        //         || panic!(),
        //         |self_| {
        //             match len {
        //                 1 => {
        //                     // TODO: investigate todo in go-hamt-ipld
        //                     if self_.cache().borrow().unwrap().pointers[0].is_shard() {
        //                         return self_;
        //                     }

        //                     self_
        //                         .cache_move()
        //                         .into_inner()
        //                         .unwrap()
        //                         .pointers
        //                         .into_iter()
        //                         .nth(0)
        //                         .unwrap()
        //                 }
        //                 2..=MAX_ARRAY_WIDTH => {
        //                     let (total_lens, has_shards): (Vec<_>, Vec<_>) = self_
        //                         .cache()
        //                         .borrow()
        //                         .unwrap()
        //                         .pointers
        //                         .iter()
        //                         .map(|p| match p {
        //                             Pointer::Link { .. } => (0, true),
        //                             Pointer::Values(v) => (v.len(), false),
        //                         })
        //                         .unzip();

        //                     let total_len: usize = total_lens.iter().sum();
        //                     let has_shards = has_shards.into_iter().any(|a| a);

        //                     if total_len >= MAX_ARRAY_WIDTH || has_shards {
        //                         return self_;
        //                     }

        //                     let chvals = self_
        //                         .cache_move()
        //                         .into_inner()
        //                         .unwrap()
        //                         .pointers
        //                         .into_iter()
        //                         .map(|p| match p {
        //                             Pointer::Link { .. } => vec![],
        //                             Pointer::Values(v) => v,
        //                         })
        //                         .flatten()
        //                         .collect();

        //                     Pointer::from_kvpairs(chvals)
        //                 }
        //                 _ => self_,
        //             }
        //         },
        //     );
        Ok(())
    }
}
