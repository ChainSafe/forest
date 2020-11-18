use super::BlockStore;
use cid::{Cid, Code};
use db::sled::{Batch, SledDb};
use encoding::{ser::Serialize, to_vec};
use std::error::Error as StdError;

impl BlockStore for SledDb {
    fn put_raw(&self, bytes: Vec<u8>, code: Code) -> Result<Cid, Box<dyn StdError>> {
        let cid = cid::new_from_cbor(&bytes, code);
        // Can do a unique compare and swap here, should only need to write when entry doesn't
        // exist as all Cids "should" be unique. If the value exists, ignore.
        let _ = self.db
            .compare_and_swap(cid.to_bytes(), None as Option<&[u8]>, Some(bytes))?;
        Ok(cid)
    }
    fn bulk_put<'a, S, V>(&self, values: V, code: Code) -> Result<Vec<Cid>, Box<dyn StdError>>
    where
        S: Serialize + 'a,
        V: IntoIterator<Item = &'a S>,
    {
        let mut batch = Batch::default();
        let cids: Vec<Cid> = values
            .into_iter()
            .map(|v| {
                let bz = to_vec(v)?;
                let cid = cid::new_from_cbor(&bz, code);
                batch.insert(cid.to_bytes(), bz);
                Ok(cid)
            })
            .collect::<Result<_, Box<dyn StdError>>>()?;
        self.db.apply_batch(batch)?;

        Ok(cids)
    }
}
