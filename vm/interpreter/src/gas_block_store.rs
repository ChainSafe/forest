// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::gas_tracker::{GasTracker, PriceList};
use cid::{Cid, Code};
use db::{Error, Store};
use forest_encoding::{de::DeserializeOwned, ser::Serialize, to_vec};
use ipld_blockstore::BlockStore;
use std::cell::RefCell;
use std::error::Error as StdError;
use std::rc::Rc;

/// Blockstore wrapper to charge gas on reads and writes
pub(crate) struct GasBlockStore<'bs, BS> {
    pub price_list: PriceList,
    pub gas: Rc<RefCell<GasTracker>>,
    pub store: &'bs BS,
}

impl<BS> BlockStore for GasBlockStore<'_, BS>
where
    BS: BlockStore,
{
    fn get<T>(&self, cid: &Cid) -> Result<Option<T>, Box<dyn StdError>>
    where
        T: DeserializeOwned,
    {
        self.gas
            .borrow_mut()
            .charge_gas(self.price_list.on_ipld_get())?;
        self.store.get(cid)
    }

    fn put<S>(&self, obj: &S, code: Code) -> Result<Cid, Box<dyn StdError>>
    where
        S: Serialize,
    {
        let bytes = to_vec(obj)?;
        self.gas
            .borrow_mut()
            .charge_gas(self.price_list.on_ipld_put(bytes.len()))?;

        self.store.put_raw(bytes, code)
    }
}

impl<BS> Store for GasBlockStore<'_, BS>
where
    BS: BlockStore,
{
    fn read<K>(&self, key: K) -> Result<Option<Vec<u8>>, Error>
    where
        K: AsRef<[u8]>,
    {
        self.store.read(key)
    }
    fn write<K, V>(&self, key: K, value: V) -> Result<(), Error>
    where
        K: AsRef<[u8]>,
        V: AsRef<[u8]>,
    {
        self.store.write(key, value)
    }
    fn delete<K>(&self, key: K) -> Result<(), Error>
    where
        K: AsRef<[u8]>,
    {
        self.store.delete(key)
    }
    fn exists<K>(&self, key: K) -> Result<bool, Error>
    where
        K: AsRef<[u8]>,
    {
        self.store.exists(key)
    }
    fn bulk_read<K>(&self, keys: &[K]) -> Result<Vec<Option<Vec<u8>>>, Error>
    where
        K: AsRef<[u8]>,
    {
        self.store.bulk_read(keys)
    }
    fn bulk_write<K, V>(&self, values: &[(K, V)]) -> Result<(), Error>
    where
        K: AsRef<[u8]>,
        V: AsRef<[u8]>,
    {
        self.store.bulk_write(values)
    }
    fn bulk_delete<K>(&self, keys: &[K]) -> Result<(), Error>
    where
        K: AsRef<[u8]>,
    {
        self.store.bulk_delete(keys)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::price_list_by_epoch;
    use cid::Code::Blake2b256;
    use db::MemoryDB;
    use vm::{ActorError, ExitCode};

    #[test]
    fn gas_blockstore() {
        let db = MemoryDB::default();
        let gbs = GasBlockStore {
            price_list: PriceList {
                ipld_get_base: 4,
                ipld_put_base: 2,
                ipld_put_per_byte: 1,
                ..price_list_by_epoch(0)
            },
            gas: Rc::new(RefCell::new(GasTracker::new(5000, 0))),
            store: &db,
        };
        assert_eq!(gbs.gas.borrow().gas_used(), 0);
        assert_eq!(to_vec(&200u8).unwrap().len(), 2);
        let c = gbs.put(&200u8, Blake2b256).unwrap();
        assert_eq!(gbs.gas.borrow().gas_used(), 2002);
        gbs.get::<u8>(&c).unwrap();
        assert_eq!(gbs.gas.borrow().gas_used(), 2006);
    }

    #[test]
    fn gas_blockstore_oog() {
        let db = MemoryDB::default();
        let gbs = GasBlockStore {
            price_list: PriceList {
                ipld_put_base: 12,
                ..price_list_by_epoch(0)
            },
            gas: Rc::new(RefCell::new(GasTracker::new(10, 0))),
            store: &db,
        };
        assert_eq!(gbs.gas.borrow().gas_used(), 0);
        assert_eq!(to_vec(&200u8).unwrap().len(), 2);
        assert_eq!(
            gbs.put(&200u8, Blake2b256)
                .unwrap_err()
                .downcast::<ActorError>()
                .unwrap()
                .exit_code(),
            ExitCode::SysErrOutOfGas
        );
    }
}
