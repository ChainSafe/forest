// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::gas_tracker::PriceList;
use cid::{Cid, Code};
use db::{Error, Store};
use forest_encoding::{de::DeserializeOwned, ser::Serialize, to_vec};
use fvm::gas::{GasCharge, GasTracker};
use fvm::kernel::ExecutionError;
use ipld_blockstore::BlockStore;
use std::cell::RefCell;
use std::error::Error as StdError;
use std::rc::Rc;

// FIXME: remove when error handling has moved to anyhow::Error. Tracking issue: https://github.com/ChainSafe/forest/issues/1536 ?
pub fn to_std_error(exec_error: ExecutionError) -> Box<dyn StdError> {
    exec_error.to_string().into()
}

pub fn to_anyhow_error(exec_error: ExecutionError) -> anyhow::Error {
    match exec_error {
        ExecutionError::OutOfGas => anyhow::Error::msg("OutOfGas"),
        ExecutionError::Syscall(err) => anyhow::Error::msg(err.to_string()),
        ExecutionError::Fatal(err) => err,
    }
}

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
        let gas_charge = GasCharge::from(self.price_list.on_ipld_get());
        self.gas
            .borrow_mut()
            .apply_charge(gas_charge)
            .map_err(to_std_error)?;
        self.store.get(cid)
    }

    fn get_bytes(&self, cid: &Cid) -> Result<Option<Vec<u8>>, Box<dyn StdError>> {
        let gas_charge = GasCharge::from(self.price_list.on_ipld_get());
        self.gas
            .borrow_mut()
            .apply_charge(gas_charge)
            .map_err(to_std_error)?;
        self.store.get_bytes(cid)
    }

    fn get_anyhow<T>(&self, cid: &Cid) -> anyhow::Result<Option<T>>
    where
        T: DeserializeOwned,
    {
        let gas_charge = GasCharge::from(self.price_list.on_ipld_get());
        self.gas
            .borrow_mut()
            .apply_charge(gas_charge)
            .map_err(to_anyhow_error)?;
        self.store.get_anyhow(cid)
    }

    fn put<S>(&self, obj: &S, code: Code) -> Result<Cid, Box<dyn StdError>>
    where
        S: Serialize,
    {
        let bytes = to_vec(obj)?;
        let gas_charge = GasCharge::from(self.price_list.on_ipld_put(bytes.len()));
        self.gas
            .borrow_mut()
            .apply_charge(gas_charge)
            .map_err(to_std_error)?;
        self.store.put_raw(bytes, code)
    }

    fn put_raw(&self, bytes: Vec<u8>, code: Code) -> Result<Cid, Box<dyn StdError>> {
        let gas_charge = GasCharge::from(self.price_list.on_ipld_put(bytes.len()));
        self.gas
            .borrow_mut()
            .apply_charge(gas_charge)
            .map_err(to_std_error)?;
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
    use fvm::gas::Gas;
    use networks::{ChainConfig, Height};

    #[test]
    fn gas_blockstore() {
        let calico_height = ChainConfig::default().epoch(Height::Calico);
        let db = MemoryDB::default();
        let gbs = GasBlockStore {
            price_list: PriceList {
                ipld_get_base: 4,
                ipld_put_base: 2,
                ipld_put_per_byte: 1,
                ..price_list_by_epoch(0, calico_height)
            },
            gas: Rc::new(RefCell::new(GasTracker::new(Gas::new(5000), Gas::new(0)))),
            store: &db,
        };
        assert_eq!(gbs.gas.borrow().gas_used(), Gas::new(0));
        assert_eq!(to_vec(&200u8).unwrap().len(), 2);
        let c = gbs.put(&200u8, Blake2b256).unwrap();
        assert_eq!(gbs.gas.borrow().gas_used(), Gas::new(2002));
        gbs.get::<u8>(&c).unwrap();
        assert_eq!(gbs.gas.borrow().gas_used(), Gas::new(2006));
    }

    #[test]
    fn gas_blockstore_oog() {
        let calico_height = ChainConfig::default().epoch(Height::Calico);
        let db = MemoryDB::default();
        let gbs = GasBlockStore {
            price_list: PriceList {
                ipld_put_base: 12,
                ..price_list_by_epoch(0, calico_height)
            },
            gas: Rc::new(RefCell::new(GasTracker::new(Gas::new(10), Gas::new(0)))),
            store: &db,
        };
        assert_eq!(gbs.gas.borrow().gas_used(), Gas::new(0));
        assert_eq!(to_vec(&200u8).unwrap().len(), 2);
        assert_eq!(
            gbs.put(&200u8, Blake2b256).unwrap_err().to_string(),
            "OutOfGas".to_string()
        );
    }
}
