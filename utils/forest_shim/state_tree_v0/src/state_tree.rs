use std::{cell::RefCell, collections::HashMap};

use address::Address;
use cid::Cid;
use forest_hash_utils::BytesKey;
// use ipld_blockstore::BlockStore;
use ipld_hamt::Hamt;
use num_bigint::bigint_ser;
use serde_repr::{Deserialize_repr, Serialize_repr};
use serde_tuple::{Deserialize_tuple, Serialize_tuple};

type TokenAmount = num_bigint::BigInt;

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
    #[serde(with = "bigint_ser")]
    pub balance: TokenAmount,
}

/// Specifies the version of the state tree
#[derive(Debug, PartialEq, Clone, Copy, PartialOrd, Serialize_repr, Deserialize_repr)]
#[repr(u64)]
pub enum StateTreeVersion {
    /// Corresponds to actors < v2
    V0,
    /// Corresponds to actors = v2
    V1,
    /// Corresponds to actors = v3
    V2,
    /// Corresponds to actors = v4
    V3,
    /// Corresponds to actors >= v5
    V4,
}

/// State root information. Contains information about the version of the state tree,
/// the root of the tree, and a link to the information about the tree.
#[derive(Deserialize_tuple, Serialize_tuple)]
pub struct StateRoot {
    /// State tree version
    pub version: StateTreeVersion,

    /// Actors tree. The structure depends on the state root version.
    pub actors: Cid,

    /// Info. The structure depends on the state root version.
    pub info: Cid,
}

pub type Map<BS, V> = Hamt<BS, V, BytesKey>;

/// State tree implementation using hamt. This structure is not threadsafe and should only be used
/// in sync contexts.
pub struct StateTree<S> {
    hamt: Map<S, ActorState>,

    version: StateTreeVersion,
    info: Option<Cid>,

    /// State cache
    snaps: StateSnapshots,
}

/// Collection of state snapshots
struct StateSnapshots {
    layers: Vec<StateSnapLayer>,
}

impl StateSnapshots {
    /// State snapshot constructor
    fn new() -> Self {
        Self {
            layers: vec![StateSnapLayer::default()],
        }
    }
}

/// State snap shot layer
#[derive(Debug, Default)]
struct StateSnapLayer {
    actors: RefCell<HashMap<Address, Option<ActorState>>>,
    resolve_cache: RefCell<HashMap<Address, Address>>,
}

use encoding::error::Error as CborError;
// use thiserror::Error;

/// Database error
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Invalid bulk write kv lengths, must be equal")]
    InvalidBulkLen,
    #[error("Cannot use unopened database")]
    Unopened,
    #[cfg(feature = "rocksdb")]
    #[error(transparent)]
    Database(#[from] rocksdb::Error),
    #[cfg(feature = "sled")]
    #[error(transparent)]
    Sled(#[from] sled::Error),
    #[error(transparent)]
    Encoding(#[from] CborError),
    #[error("{0}")]
    Other(String),
}

/// Store interface used as a KV store implementation
pub trait Store {
    /// Read single value from data store and return `None` if key doesn't exist.
    fn read<K>(&self, key: K) -> Result<Option<Vec<u8>>, Error>
    where
        K: AsRef<[u8]>;

    /// Write a single value to the data store.
    fn write<K, V>(&self, key: K, value: V) -> Result<(), Error>
    where
        K: AsRef<[u8]>,
        V: AsRef<[u8]>;

    /// Delete value at key.
    fn delete<K>(&self, key: K) -> Result<(), Error>
    where
        K: AsRef<[u8]>;

    /// Returns `Ok(true)` if key exists in store
    fn exists<K>(&self, key: K) -> Result<bool, Error>
    where
        K: AsRef<[u8]>;

    /// Read slice of keys and return a vector of optional values.
    fn bulk_read<K>(&self, keys: &[K]) -> Result<Vec<Option<Vec<u8>>>, Error>
    where
        K: AsRef<[u8]>,
    {
        keys.iter().map(|key| self.read(key)).collect()
    }

    /// Write slice of KV pairs.
    fn bulk_write<K, V>(&self, values: &[(K, V)]) -> Result<(), Error>
    where
        K: AsRef<[u8]>,
        V: AsRef<[u8]>,
    {
        values
            .iter()
            .try_for_each(|(key, value)| self.write(key, value))
    }

    /// Bulk delete keys from the data store.
    fn bulk_delete<K>(&self, keys: &[K]) -> Result<(), Error>
    where
        K: AsRef<[u8]>,
    {
        keys.iter().try_for_each(|key| self.delete(key))
    }
}

use cid::Code;
// use db::{MemoryDB, Store};
use encoding::{de::DeserializeOwned, from_slice, ser::Serialize, to_vec};
use std::error::Error as StdError;

/// Wrapper for database to handle inserting and retrieving ipld data with Cids
pub trait BlockStore: Store {
    /// Get bytes from block store by Cid.
    fn get_bytes(&self, cid: &Cid) -> Result<Option<Vec<u8>>, Box<dyn StdError>> {
        Ok(self.read(cid.to_bytes())?)
    }

    /// Get typed object from block store by Cid.
    fn get<T>(&self, cid: &Cid) -> Result<Option<T>, Box<dyn StdError>>
    where
        T: DeserializeOwned,
    {
        match self.get_bytes(cid)? {
            Some(bz) => Ok(Some(from_slice(&bz)?)),
            None => Ok(None),
        }
    }

    /// Put an object in the block store and return the Cid identifier.
    fn put<S>(&self, obj: &S, code: Code) -> Result<Cid, Box<dyn StdError>>
    where
        S: Serialize,
    {
        let bytes = to_vec(obj)?;
        self.put_raw(bytes, code)
    }

    /// Put raw bytes in the block store and return the Cid identifier.
    fn put_raw(&self, bytes: Vec<u8>, code: Code) -> Result<Cid, Box<dyn StdError>> {
        let cid = cid::new_from_cbor(&bytes, code);
        self.write(cid.to_bytes(), bytes)?;
        Ok(cid)
    }

    /// Batch put cbor objects into blockstore and returns vector of Cids
    fn bulk_put<'a, S, V>(&self, values: V, code: Code) -> Result<Vec<Cid>, Box<dyn StdError>>
    where
        S: Serialize + 'a,
        V: IntoIterator<Item = &'a S>,
    {
        values
            .into_iter()
            .map(|value| self.put(value, code))
            .collect()
    }
}

impl<S> StateTree<S>
where
    S: BlockStore,
{
    // pub fn new(store: &'db S, version: StateTreeVersion) -> Result<Self, Box<dyn StdError>> {
    //     let info = match version {
    //         StateTreeVersion::V0 => None,
    //         StateTreeVersion::V1
    //         | StateTreeVersion::V2
    //         | StateTreeVersion::V3
    //         | StateTreeVersion::V4 => Some(store.put(&StateInfo0::default(), Blake2b256)?),
    //     };

    //     let hamt = Map::new(store, ActorVersion::from(version));
    //     Ok(Self {
    //         hamt,
    //         version,
    //         info,
    //         snaps: StateSnapshots::new(),
    //     })
    // }

    /// Constructor for a hamt state tree given an IPLD store
    pub fn new_from_root(store: S, c: &Cid) -> Result<Self, Box<dyn std::error::Error>> {
        // Try to load state root, if versioned
        let (version, info, actors) = if let Ok(Some(StateRoot {
            version,
            info,
            actors,
        })) = store.get(&c)
        {
            (version, Some(info), actors)
        } else {
            // Fallback to v0 state tree if retrieval fails
            (StateTreeVersion::V0, None, *c)
        };

        // match version {
        //     StateTreeVersion::V0
        //     | StateTreeVersion::V1
        //     | StateTreeVersion::V2
        //     | StateTreeVersion::V3
        //     | StateTreeVersion::V4 => {
        //         let hamt = Map::load_with_bit_width(&actors, store, version.into())?;

        //         Ok(Self {
        //             hamt,
        //             version,
        //             info,
        //             snaps: StateSnapshots::new(),
        //         })
        //     }
        // }
        todo!()
    }

    // /// Imports a StateTree given an AsyncRead that has data in CAR format
    // pub async fn import_state_tree<R: AsyncRead + Send + Unpin>(
    //     store: &'db S,
    //     reader: R,
    // ) -> Result<Cid, String> {
    //     let state_root = forest_car::load_car(store, reader)
    //         .await
    //         .map_err(|e| format!("Import StateTree failed: {}", e.to_string()))?;
    //     if state_root.len() != 1 {
    //         return Err(format!(
    //             "Import StateTree failed: expected root length of 1, got: {}",
    //             state_root.len()
    //         ));
    //     }

    //     // Safe to grab index 0 because we have verified that the length of the vec is exactly 1
    //     let state_root = state_root[0];

    //     // Attempt to load StateTree to see if the root CID indeed points to a valid StateTree
    //     StateTree::new_from_root(store, &state_root).map_err(|e| {
    //         format!(
    //             "Import StateTree failed: Invalid StateTree root: {}",
    //             e.to_string()
    //         )
    //     })?;

    //     Ok(state_root)
    // }

    // /// Retrieve store reference to modify db.
    // pub fn store(&self) -> &S {
    //     self.hamt.store()
    // }

    // /// Get actor state from an address. Will be resolved to ID address.
    // pub fn get_actor(&self, addr: &Address) -> Result<Option<ActorState>, Box<dyn StdError>> {
    //     let addr = match self.lookup_id(addr)? {
    //         Some(addr) => addr,
    //         None => return Ok(None),
    //     };

    //     // Check cache for actor state
    //     if let Some(actor_state) = self.snaps.get_actor(&addr) {
    //         return Ok(Some(actor_state));
    //     }

    //     // if state doesn't exist, find using hamt
    //     let act = self.hamt.get(&addr.to_bytes())?.cloned();

    //     // Update cache if state was found
    //     if let Some(act_s) = &act {
    //         self.snaps.set_actor(addr, act_s.clone())?;
    //     }

    //     Ok(act)
    // }

    // /// Set actor state for an address. Will set state at ID address.
    // pub fn set_actor(
    //     &mut self,
    //     addr: &Address,
    //     actor: ActorState,
    // ) -> Result<(), Box<dyn StdError>> {
    //     let addr = self
    //         .lookup_id(addr)?
    //         .ok_or_else(|| format!("Resolution lookup failed for {}", addr))?;

    //     self.snaps.set_actor(addr, actor)
    // }

    // /// Get an ID address from any Address
    // pub fn lookup_id(&self, addr: &Address) -> Result<Option<Address>, Box<dyn StdError>> {
    //     if addr.protocol() == Protocol::ID {
    //         return Ok(Some(*addr));
    //     }

    //     if let Some(res_address) = self.snaps.resolve_address(addr) {
    //         return Ok(Some(res_address));
    //     }

    //     let init_act = self
    //         .get_actor(actor::init::ADDRESS)?
    //         .ok_or("Init actor address could not be resolved")?;

    //     let state = init::State::load(self.hamt.store(), &init_act)?;

    //     let a: Address = match state
    //         .resolve_address(self.store(), addr)
    //         .map_err(|e| format!("Could not resolve address: {:?}", e))?
    //     {
    //         Some(a) => a,
    //         None => return Ok(None),
    //     };

    //     self.snaps.cache_resolve_address(*addr, a)?;

    //     Ok(Some(a))
    // }

    // /// Delete actor for an address. Will resolve to ID address to delete.
    // pub fn delete_actor(&mut self, addr: &Address) -> Result<(), Box<dyn StdError>> {
    //     let addr = self
    //         .lookup_id(addr)?
    //         .ok_or_else(|| format!("Resolution lookup failed for {}", addr))?;

    //     // Remove value from cache
    //     self.snaps.delete_actor(addr)?;

    //     Ok(())
    // }

    // /// Mutate and set actor state for an Address.
    // pub fn mutate_actor<F>(&mut self, addr: &Address, mutate: F) -> Result<(), Box<dyn StdError>>
    // where
    //     F: FnOnce(&mut ActorState) -> Result<(), String>,
    // {
    //     // Retrieve actor state from address
    //     let mut act: ActorState = self
    //         .get_actor(addr)?
    //         .ok_or(format!("Actor for address: {} does not exist", addr))?;

    //     // Apply function of actor state
    //     mutate(&mut act)?;
    //     // Set the actor
    //     self.set_actor(addr, act)
    // }

    // /// Register a new address through the init actor.
    // pub fn register_new_address(&mut self, addr: &Address) -> Result<Address, Box<dyn StdError>> {
    //     let mut actor: ActorState = self
    //         .get_actor(init::ADDRESS)?
    //         .ok_or("Could not retrieve init actor")?;

    //     let mut ias = init::State::load(self.store(), &actor)?;

    //     let new_addr = ias.map_address_to_new_id(self.store(), addr)?;

    //     // Set state for init actor in store and update root Cid
    //     actor.state = self.store().put(&ias, Blake2b256)?;

    //     self.set_actor(init::ADDRESS, actor)?;

    //     Ok(new_addr)
    // }

    // /// Add snapshot layer to stack.
    // pub fn snapshot(&mut self) -> Result<(), String> {
    //     self.snaps.add_layer();
    //     Ok(())
    // }

    // /// Merges last two snap shot layers.
    // pub fn clear_snapshot(&mut self) -> Result<(), String> {
    //     self.snaps.merge_last_layer()
    // }

    // /// Revert state cache by removing last snapshot
    // pub fn revert_to_snapshot(&mut self) -> Result<(), String> {
    //     self.snaps.drop_layer()?;
    //     self.snaps.add_layer();
    //     Ok(())
    // }

    // /// Flush state tree and return Cid root.
    // pub fn flush(&mut self) -> Result<Cid, Box<dyn StdError>> {
    //     if self.snaps.layers.len() != 1 {
    //         return Err(format!(
    //             "tried to flush state tree with snapshots on the stack: {:?}",
    //             self.snaps.layers.len()
    //         )
    //         .into());
    //     }

    //     for (addr, sto) in self.snaps.layers[0].actors.borrow().iter() {
    //         match sto {
    //             None => {
    //                 self.hamt.delete(&addr.to_bytes())?;
    //             }
    //             Some(ref state) => {
    //                 self.hamt.set(addr.to_bytes().into(), state.clone())?;
    //             }
    //         }
    //     }

    //     let root = self.hamt.flush()?;

    //     if matches!(self.version, StateTreeVersion::V0) {
    //         Ok(root)
    //     } else {
    //         Ok(self.store().put(
    //             &StateRoot {
    //                 version: self.version,
    //                 actors: root,
    //                 info: self
    //                     .info
    //                     .expect("malformed state tree, version 1 and version 2 require info"),
    //             },
    //             Blake2b256,
    //         )?)
    //     }
    // }

    // pub fn for_each<F>(&self, mut f: F) -> Result<(), Box<dyn StdError>>
    // where
    //     F: FnMut(Address, &ActorState) -> Result<(), Box<dyn StdError>>,
    //     S: BlockStore,
    // {
    //     self.hamt.for_each(|k, v| f(Address::from_bytes(&k.0)?, v))
    // }
}
