// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::node::Node;
use super::{Error, KeyValuePair, MAX_ARRAY_WIDTH};
use cid::Cid;
use ipld_blockstore::BlockStore;
use lazycell::AtomicLazyCell;
use replace_with::replace_with;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

// TODO: make Pointer an enum once things are working
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(bound(deserialize = "KeyValuePair<K, V>: DeserializeOwned"))]
pub(crate) struct Pointer<K, V> {
    #[serde(rename = "v", skip_serializing_if = "Vec::is_empty")]
    pub(crate) kvs: Vec<KeyValuePair<K, V>>,
    #[serde(rename = "l", skip_serializing_if = "Option::is_none")]
    link: Option<Cid>,
    #[serde(skip)]
    cache: AtomicLazyCell<Node<K, V>>,
}

impl<K: PartialEq, V: PartialEq> PartialEq for Pointer<K, V> {
    fn eq(&self, other: &Self) -> bool {
        self.kvs == other.kvs && self.link == other.link
    }
}

impl<K: Eq, V: Eq> Eq for Pointer<K, V> {}

impl<K, V> Default for Pointer<K, V> {
    fn default() -> Self {
        Pointer {
            kvs: Vec::new(),
            link: None,
            cache: AtomicLazyCell::new(),
        }
    }
}

impl<K, V> Pointer<K, V>
where
    K: Serialize + DeserializeOwned,
    V: Serialize + DeserializeOwned,
{
    pub fn from_link(link: Cid, node: Node<K, V>) -> Self {
        let cache = AtomicLazyCell::new();
        cache.fill(node).map_err(|_| ()).unwrap();

        Pointer {
            kvs: Vec::new(),
            link: Some(link),
            cache,
        }
    }

    pub fn from_key_value(key: K, value: V) -> Self {
        Pointer {
            kvs: vec![KeyValuePair::new(key, value)],
            link: None,
            cache: AtomicLazyCell::new(),
        }
    }

    pub fn from_kvpairs(kvs: Vec<KeyValuePair<K, V>>) -> Self {
        Pointer {
            kvs,
            link: None,
            cache: AtomicLazyCell::new(),
        }
    }

    pub fn is_shard(&self) -> bool {
        self.link.is_some()
    }

    pub(crate) fn load_child<S: BlockStore>(&self, store: &S) -> Result<&Node<K, V>, Error> {
        if !self.cache.filled() {
            if let Some(ref link) = self.link {
                match store.get(link)? {
                    Some(node) => {
                        self.cache.fill(node).map_err(|_| ()).unwrap();
                    }
                    None => return Err(Error::Custom("node not found")),
                }
            } else {
                return Err(Error::Custom("Cannot load child from non link node"));
            }
        }
        Ok(self.cache.borrow().unwrap())
    }

    pub(crate) fn load_child_mut<S: BlockStore>(
        &mut self,
        store: &S,
    ) -> Result<&mut Node<K, V>, Error> {
        if !self.cache.filled() {
            if let Some(ref link) = self.link {
                match store.get(link)? {
                    Some(node) => {
                        self.cache.fill(node).map_err(|_| ()).unwrap();
                    }
                    None => return Err(Error::Custom("node not found")),
                }
            } else {
                return Err(Error::Custom("Cannot load child from non link node"));
            }
        }
        Ok(self.cache.borrow_mut().unwrap())
    }

    /// Internal method to cleanup children, to ensure consistent tree representation
    /// after deletes.
    pub fn clean(&mut self) -> Result<(), Error> {
        assert!(self.cache.filled());
        let len = self.cache.borrow().unwrap().pointers.len();
        if len == 0 {
            return Err(Error::Custom("Invalid HAMT"));
        }

        replace_with(
            self,
            || panic!(),
            |self_| {
                match len {
                    1 => {
                        // TODO: investigate todo in go-hamt-ipld
                        if self_.cache.borrow().unwrap().pointers[0].is_shard() {
                            return self_;
                        }

                        self_
                            .cache
                            .into_inner()
                            .unwrap()
                            .pointers
                            .into_iter()
                            .nth(0)
                            .unwrap()
                    }
                    2..=MAX_ARRAY_WIDTH => {
                        let (total_lens, has_shards): (Vec<_>, Vec<_>) = self_
                            .cache
                            .borrow()
                            .unwrap()
                            .pointers
                            .iter()
                            .map(|p| (p.kvs.len(), p.is_shard()))
                            .unzip();

                        let total_len: usize = total_lens.iter().sum();
                        let has_shards = has_shards.into_iter().any(|a| a);

                        if total_len >= MAX_ARRAY_WIDTH || has_shards {
                            return self_;
                        }

                        let chvals = self_
                            .cache
                            .into_inner()
                            .unwrap()
                            .pointers
                            .into_iter()
                            .map(|p| p.kvs)
                            .flatten()
                            .collect();

                        Pointer::from_kvpairs(chvals)
                    }
                    _ => self_,
                }
            },
        );
        Ok(())
    }
}
