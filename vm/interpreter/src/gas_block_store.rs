// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use cid::Cid;
use forest_db::{Error, Store};
use fvm::gas::{GasTracker, PriceList};
use fvm_ipld_blockstore::Blockstore;
use std::cell::RefCell;
use std::rc::Rc;

/// `BlockStore` wrapper to charge gas on reads and writes
pub(crate) struct GasBlockStore<'bs, BS> {
    pub price_list: PriceList,
    pub gas: Rc<RefCell<GasTracker>>,
    pub store: &'bs BS,
}

impl<BS: Store> Store for GasBlockStore<'_, BS> {
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

impl<BS> Blockstore for GasBlockStore<'_, BS>
where
    BS: Blockstore,
{
    fn get(&self, k: &Cid) -> anyhow::Result<Option<Vec<u8>>> {
        let gas_charge = self.price_list.on_block_open_base();
        self.gas.borrow_mut().apply_charge(gas_charge)?;
        self.store.get(k)
    }

    fn put_keyed(&self, k: &Cid, block: &[u8]) -> anyhow::Result<()> {
        let gas_charge = self.price_list.on_block_link(block.len());
        self.gas.borrow_mut().apply_charge(gas_charge)?;
        self.store.put_keyed(k, block)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cid::multihash::Code::Blake2b256;
    use forest_db::MemoryDB;
    use fvm::gas::{price_list_by_network_version, Gas};
    use fvm_ipld_encoding::to_vec;
    use ipld_blockstore::BlockStoreExt;
    use networks::{ChainConfig, Height};

    #[test]
    fn gas_blockstore() {
        let calico_height = ChainConfig::default().epoch(Height::Calico);
        let network_version = ChainConfig::default().network_version(calico_height);
        let db = MemoryDB::default();
        let price_list = price_list_by_network_version(network_version).clone();
        let gbs = GasBlockStore {
            price_list: price_list.clone(),
            gas: Rc::new(RefCell::new(GasTracker::new(
                Gas::new(i64::MAX),
                Gas::new(0),
            ))),
            store: &db,
        };
        assert_eq!(gbs.gas.borrow().gas_used(), Gas::new(0));
        assert_eq!(to_vec(&200u8).unwrap().len(), 2);
        let c = gbs.put_obj(&200u8, Blake2b256).unwrap();
        let put_gas = price_list.on_block_link(2).total();
        assert_eq!(gbs.gas.borrow().gas_used(), put_gas);
        gbs.get_obj::<u8>(&c).unwrap();
        let get_gas = price_list.on_block_open_base().total();
        assert_eq!(gbs.gas.borrow().gas_used(), put_gas + get_gas);
    }

    #[test]
    fn gas_blockstore_oog() {
        let calico_height = ChainConfig::default().epoch(Height::Calico);
        let network_version = ChainConfig::default().network_version(calico_height);
        let db = MemoryDB::default();
        let gbs = GasBlockStore {
            price_list: price_list_by_network_version(network_version).clone(),
            gas: Rc::new(RefCell::new(GasTracker::new(Gas::new(10), Gas::new(0)))),
            store: &db,
        };
        assert_eq!(gbs.gas.borrow().gas_used(), Gas::new(0));
        assert_eq!(to_vec(&200u8).unwrap().len(), 2);
        assert_eq!(
            &gbs.put_obj(&200u8, Blake2b256).unwrap_err().to_string(),
            "out of gas"
        );
    }
}
