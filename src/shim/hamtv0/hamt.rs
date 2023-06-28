// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::error::Error;
use super::hash_algorithm::{HashAlgorithm, Sha256};
use super::node::Node;
use cid::Cid;
use forest_hash_utils::{BytesKey, Hash};
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::tuple::*;
use fvm_ipld_encoding::tuple::{Deserialize_tuple, Serialize_tuple};
use fvm_ipld_encoding::CborStore;
use serde::{de::DeserializeOwned, Serialize, Serializer};
use std::borrow::Borrow;
use std::marker::PhantomData;

/// State of all actor implementations.
#[derive(PartialEq, Eq, Clone, Debug, Serialize_tuple, Deserialize_tuple)]
pub struct ActorState {
    /// Link to code for the actor.
    pub code: Cid,
    /// Link to the state of the actor.
    pub state: Cid,
    /// Sequence of the actor.
    pub sequence: u64,
    /// Tokens available to the actor.
    pub balance: fvm_shared3::econ::TokenAmount,
}

/// Implementation of the HAMT data structure for IPLD.
///
/// # Examples
///
/// ```
/// use ipld_hamt::Hamt;
///
/// let store = db::MemoryDB::default();
///
/// let mut map: Hamt<_, _, usize> = Hamt::new(&store);
/// map.set(1, "a".to_string()).unwrap();
/// assert_eq!(map.get(&1).unwrap(), Some(&"a".to_string()));
/// assert_eq!(map.delete(&1).unwrap(), Some((1, "a".to_string())));
/// assert_eq!(map.get::<_>(&1).unwrap(), None);
/// let cid = map.flush().unwrap();
/// ```
#[derive(Debug)]
pub struct Hamt<BS, V, K = BytesKey, H = Sha256> {
    root: Node<K, V, H>,
    store: BS,

    bit_width: u32,
    hash: PhantomData<H>,
}

impl<BS, V, K, H> Serialize for Hamt<BS, V, K, H>
where
    K: Serialize,
    V: Serialize,
    H: HashAlgorithm,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.root.serialize(serializer)
    }
}

impl<K: PartialEq, V: PartialEq, S: Blockstore, H: HashAlgorithm> PartialEq for Hamt<S, V, K, H> {
    fn eq(&self, other: &Self) -> bool {
        self.root == other.root
    }
}

impl<BS, V, K, H> Hamt<BS, V, K, H>
where
    K: Hash + Eq + PartialOrd + Serialize + DeserializeOwned,
    V: Serialize + DeserializeOwned,
    BS: Blockstore,
    H: HashAlgorithm,
{
    /// Lazily instantiate a HAMT from this root Cid with a specified bit width.
    pub fn load_with_bit_width(cid: &cid::Cid, store: BS, bit_width: u32) -> Result<Self, Error> {
        match store.get_cbor(cid)? {
            Some(root) => Ok(Self {
                root,
                store,
                bit_width,
                hash: Default::default(),
            }),
            None => Err(Error::CidNotFound(cid.to_string())),
        }
    }

    /// Returns a reference to the underlying store of the HAMT.
    pub fn store(&self) -> &BS {
        &self.store
    }

    /// Returns a reference to the value corresponding to the key.
    ///
    /// The key may be any borrowed form of the map's key type, but
    /// `Hash` and `Eq` on the borrowed form *must* match those for
    /// the key type.
    ///
    /// # Examples
    ///
    /// ```
    /// use ipld_hamt::Hamt;
    ///
    /// let store = db::MemoryDB::default();
    ///
    /// let mut map: Hamt<_, _, usize> = Hamt::new(&store);
    /// map.set(1, "a".to_string()).unwrap();
    /// assert_eq!(map.get(&1).unwrap(), Some(&"a".to_string()));
    /// assert_eq!(map.get(&2).unwrap(), None);
    /// ```
    #[inline]
    pub fn get<Q: ?Sized>(&self, k: &Q) -> Result<Option<&V>, Error>
    where
        K: Borrow<Q>,
        Q: Hash + Eq,
        V: DeserializeOwned,
    {
        match self.root.get(k, self.store.borrow(), self.bit_width)? {
            Some(v) => Ok(Some(v)),
            None => Ok(None),
        }
    }
}
