use super::BlockStore;
use cid::{Cid, Code};
use db::rocks::{RocksDb, WriteBatch};
use encoding::{ser::Serialize, to_vec};
use std::error::Error as StdError;

#[cfg(feature = "rocksdb")]
impl BlockStore for RocksDb {
    fn bulk_put<'a, S, V>(&self, values: V, code: Code) -> Result<Vec<Cid>, Box<dyn StdError>>
    where
        S: Serialize + 'a,
        V: IntoIterator<Item = &'a S>,
    {
        let mut batch = WriteBatch::default();
        let cids: Vec<Cid> = values
            .into_iter()
            .map(|v| {
                let bz = to_vec(v)?;
                let cid = cid::new_from_cbor(&bz, code);
                batch.put(cid.to_bytes(), bz);
                Ok(cid)
            })
            .collect::<Result<_, Box<dyn StdError>>>()?;
        self.db.write(batch)?;

        Ok(cids)
    }
}
