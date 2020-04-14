use cid::{multihash::MultihashDigest, Cid};
use db::{Error, Store};
use forest_encoding::{de::DeserializeOwned, ser::Serialize, to_vec};
use ipld_blockstore::BlockStore;
use std::cell::RefCell;
use std::rc::Rc;
use vm::{GasTracker, PriceList};

pub(crate) struct GasBlockStore<'bs, BS> {
    pub price_list: PriceList,
    pub gas: Rc<RefCell<GasTracker>>,
    pub store: &'bs BS,
}

impl<BS> BlockStore for GasBlockStore<'_, BS>
where
    BS: BlockStore,
{
    /// Get bytes from block store by Cid
    fn get_bytes(&self, cid: &Cid) -> Result<Option<Vec<u8>>, Error> {
        // TODO investigate if should panic/exit here, should be fatal
        let ret = self.store.get_bytes(cid)?;
        if let Some(bz) = &ret {
            self.gas
                .borrow_mut()
                .charge_gas(self.price_list.on_ipld_get(bz.len()))
                .unwrap();
        }
        Ok(ret)
    }

    /// Get typed object from block store by Cid
    fn get<T>(&self, cid: &Cid) -> Result<Option<T>, Error>
    where
        T: DeserializeOwned,
    {
        self.store.get(cid)
    }

    /// Put an object in the block store and return the Cid identifier
    fn put<S, T>(&self, obj: &S, hash: T) -> Result<Cid, Error>
    where
        S: Serialize,
        T: MultihashDigest,
    {
        self.gas
            .borrow_mut()
            .charge_gas(self.price_list.on_ipld_put(to_vec(obj).unwrap().len()))
            .unwrap();

        // TODO investigate if error here should be fatal
        self.store.put(obj, hash)
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
    fn bulk_write<K, V>(&self, keys: &[K], values: &[V]) -> Result<(), Error>
    where
        K: AsRef<[u8]>,
        V: AsRef<[u8]>,
    {
        self.store.bulk_write(keys, values)
    }
    fn bulk_delete<K>(&self, keys: &[K]) -> Result<(), Error>
    where
        K: AsRef<[u8]>,
    {
        self.store.bulk_delete(keys)
    }
}
