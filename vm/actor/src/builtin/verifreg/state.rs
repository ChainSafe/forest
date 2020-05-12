use crate::HAMT_BIT_WIDTH;
use address::Address;
use cid::Cid;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use encoding::Cbor;
use ipld_blockstore::BlockStore;
use ipld_hamt::{BytesKey, Hamt};

use crate::builtin::verifreg::types::Datacap;

#[derive(Default)]
pub struct State {
    pub root_key: Address,
    pub verifiers: Cid,
    pub verified_clients: Cid,
}

impl State {
    pub fn new(empty_map: Cid, root_key: Address) -> State {
        State {
            root_key,
            verifiers: empty_map.clone(),
            verified_clients: empty_map,
        }
    }

    pub fn put_verified<BS: BlockStore>(
        &mut self,
        store: &BS,
        verified_addr: Address,
        verifier_cap: Datacap,
    ) -> Result<(), String> {
        Self::put(&mut self.verifiers, store, verified_addr, verifier_cap)
    }

    pub fn get_verifier<BS: BlockStore>(
        &mut self,
        store: &BS,
        address_get: Address,
    ) -> Result<Option<Datacap>, String> {
        Self::get(&mut self.verifiers, store, address_get)
    }

    pub fn delete_verifier<BS: BlockStore>(
        &mut self,
        store: &BS,
        address: Address,
    ) -> Result<(), String> {
        Self::delete(&mut self.verifiers, store, address)
    }

    pub fn put_verified_client<BS: BlockStore>(
        &mut self,
        store: &BS,
        verified_addr: Address,
        verifier_cap: Datacap,
    ) -> Result<(), String> {
        Self::put(
            &mut self.verified_clients,
            store,
            verified_addr,
            verifier_cap,
        )
    }

    pub fn get_verified_clients<BS: BlockStore>(
        &mut self,
        store: &BS,
        address: Address,
    ) -> Result<Option<Datacap>, String> {
        Self::get(&mut self.verified_clients, store, address)
    }

    pub fn delete_verified_clients<BS: BlockStore>(
        &mut self,
        store: &BS,
        address: Address,
    ) -> Result<(), String> {
        Self::delete(&mut self.verified_clients, store, address)
    }

    //private helper functions
    fn put<BS: BlockStore>(
        storage: &mut Cid,
        store: &BS,
        verified_addr: Address,
        verifier_cap: Datacap,
    ) -> Result<(), String> {
        let mut map: Hamt<BytesKey, _> =
            Hamt::load_with_bit_width(&storage, store, HAMT_BIT_WIDTH)?;
        map.set(verified_addr.to_bytes().into(), &verifier_cap)?;
        map.flush()?;
        Ok(())
    }

    fn get<BS: BlockStore>(
        storage: &mut Cid,
        store: &BS,
        verified_addr: Address,
    ) -> Result<Option<Datacap>, String> {
        let map: Hamt<BytesKey, _> = Hamt::load_with_bit_width(&storage, store, HAMT_BIT_WIDTH)?;
        map.get(&verified_addr.to_bytes())
            .map_err(|e| e.to_string())
    }

    fn delete<BS: BlockStore>(
        storage: &mut Cid,
        store: &BS,
        verified_addr: Address,
    ) -> Result<(), String> {
        let mut map: Hamt<BytesKey, _> =
            Hamt::load_with_bit_width(&storage, store, HAMT_BIT_WIDTH)?;
        map.delete(&verified_addr.to_bytes())?;
        map.flush()?;
        Ok(())
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
