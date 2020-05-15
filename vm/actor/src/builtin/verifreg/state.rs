// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::HAMT_BIT_WIDTH;
use address::Address;
use cid::Cid;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use encoding::Cbor;
use ipld_blockstore::BlockStore;
use ipld_hamt::{BytesKey, Hamt};
use num_bigint::biguint_ser::{BigUintDe, BigUintSer};

use crate::builtin::verifreg::types::Datacap;

pub struct State {
    pub root_key: Address,
    pub verifiers: Cid,
    pub verified_clients: Cid,
}

type StateResult<T> = Result<T, String>;
impl State {
    pub fn new(empty_map: Cid, root_key: Address) -> State {
        State {
            root_key,
            verifiers: empty_map.clone(),
            verified_clients: empty_map,
        }
    }

    pub fn put_verifier<BS: BlockStore>(
        &mut self,
        store: &BS,
        verified_addr: &Address,
        verifier_cap: &Datacap,
    ) -> StateResult<()> {
        self.verifiers = Self::put(&self.verifiers, store, verified_addr, verifier_cap)?;
        Ok(())
    }

    pub fn get_verifier<BS: BlockStore>(
        &mut self,
        store: &BS,
        address_get: &Address,
    ) -> StateResult<Option<Datacap>> {
        Self::get(&self.verifiers, store, address_get)
    }

    pub fn delete_verifier<BS: BlockStore>(
        &mut self,
        store: &BS,
        address: &Address,
    ) -> StateResult<()> {
        self.verifiers = Self::delete(&self.verifiers, store, address)?;
        Ok(())
    }

    pub fn put_verified_client<BS: BlockStore>(
        &mut self,
        store: &BS,
        verified_addr: &Address,
        verifier_cap: &Datacap,
    ) -> StateResult<()> {
        self.verified_clients =
            Self::put(&self.verified_clients, store, verified_addr, verifier_cap)?;

        Ok(())
    }

    pub fn get_verified_client<BS: BlockStore>(
        &self,
        store: &BS,
        address: &Address,
    ) -> StateResult<Option<Datacap>> {
        Self::get(&self.verified_clients, store, address)
    }

    pub fn delete_verified_client<BS: BlockStore>(
        &mut self,
        store: &BS,
        address: &Address,
    ) -> StateResult<()> {
        self.verified_clients = Self::delete(&self.verified_clients, store, address)?;
        Ok(())
    }

    //private helper functions
    fn put<BS: BlockStore>(
        storage: &Cid,
        store: &BS,
        verified_addr: &Address,
        verifier_cap: &Datacap,
    ) -> StateResult<Cid> {
        let mut map: Hamt<BytesKey, _> =
            Hamt::load_with_bit_width(&storage, store, HAMT_BIT_WIDTH)?;
        map.set(verified_addr.to_bytes().into(), BigUintSer(&verifier_cap))?;
        let root = map.flush()?;
        Ok(root)
    }

    fn get<BS: BlockStore>(
        storage: &Cid,
        store: &BS,
        verified_addr: &Address,
    ) -> StateResult<Option<Datacap>> {
        let map: Hamt<BytesKey, _> = Hamt::load_with_bit_width(&storage, store, HAMT_BIT_WIDTH)?;
        Ok(map
            .get::<_, BigUintDe>(&verified_addr.to_bytes())?
            .map(|s| s.0))
    }

    fn delete<BS: BlockStore>(
        storage: &Cid,
        store: &BS,
        verified_addr: &Address,
    ) -> StateResult<Cid> {
        let mut map: Hamt<BytesKey, _> =
            Hamt::load_with_bit_width(&storage, store, HAMT_BIT_WIDTH)?;
        map.delete(&verified_addr.to_bytes())?;
        let root = map.flush()?;
        Ok(root)
    }
}

impl Cbor for State {}

impl Serialize for State {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (&self.root_key, &self.verifiers, &self.verified_clients).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for State {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (root_key, verifiers, verified_clients) = Deserialize::deserialize(deserializer)?;
        Ok(Self {
            root_key,
            verifiers,
            verified_clients,
        })
    }
}
