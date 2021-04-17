// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#![cfg(feature = "buffered")]

use super::BlockStore;
use byteorder::{BigEndian, ByteOrder, ReadBytesExt};
use cid::{Cid, Code, DAG_CBOR};
use db::{Error, Store};
use std::collections::HashMap;
use std::error::Error as StdError;
use std::io::{Read, Seek};
use std::{cell::RefCell, convert::TryFrom, io::Cursor};

/// Wrapper around `BlockStore` to limit and have control over when values are written.
/// This type is not threadsafe and can only be used in synchronous contexts.
#[derive(Debug)]
pub struct BufferedBlockStore<'bs, BS> {
    base: &'bs BS,
    write: RefCell<HashMap<Cid, Vec<u8>>>,
}

impl<'bs, BS> BufferedBlockStore<'bs, BS>
where
    BS: BlockStore,
{
    pub fn new(base: &'bs BS) -> Self {
        Self {
            base,
            write: Default::default(),
        }
    }
    /// Flushes the buffered cache based on the root node.
    /// This will recursively traverse the cache and write all data connected by links to this
    /// root Cid.
    pub fn flush(&mut self, root: &Cid) -> Result<(), Box<dyn StdError>> {
        let mut buffer = Vec::new();
        copy_rec(self.base, &self.write.borrow(), *root, &mut buffer)?;

        self.base.bulk_write(&buffer)?;
        self.write = Default::default();

        Ok(())
    }
}

/// Given a CBOR encoded Buffer, returns a tuple of:
/// the type of the CBOR object along with extra
/// elements we expect to read. More info on this can be found in
/// Appendix C. of RFC 7049 which defines the CBOR specification.
/// This was implemented because the CBOR library we use does not expose low
/// methods like this, requiring us to deserialize the whole CBOR payload, which
/// is unnecessary and quite inefficient for our usecase here.
fn cbor_read_header_buf<B: Read>(
    br: &mut B,
    scratch: &mut [u8],
) -> Result<(u8, usize), Box<dyn StdError>> {
    let first = br.read_u8()?;
    let maj = (first & 0xe0) >> 5;
    let low = first & 0x1f;

    if low < 24 {
        Ok((maj, low as usize))
    } else if low == 24 {
        let val = br.read_u8()?;
        if val < 24 {
            return Err("cbor input was not canonical (lval 24 with value < 24)".into());
        }
        Ok((maj, val as usize))
    } else if low == 25 {
        br.read_exact(&mut scratch[..2])?;
        let val = BigEndian::read_u16(&scratch[..2]);
        if val <= u8::MAX as u16 {
            return Err("cbor input was not canonical (lval 25 with value <= MaxUint8)".into());
        }
        Ok((maj, val as usize))
    } else if low == 26 {
        br.read_exact(&mut scratch[..4])?;
        let val = BigEndian::read_u32(&scratch[..4]);
        if val <= u16::MAX as u32 {
            return Err("cbor input was not canonical (lval 26 with value <= MaxUint16)".into());
        }
        Ok((maj, val as usize))
    } else if low == 27 {
        br.read_exact(&mut scratch[..8])?;
        let val = BigEndian::read_u64(&scratch[..8]);
        if val <= u32::MAX as u64 {
            return Err("cbor input was not canonical (lval 27 with value <= MaxUint32)".into());
        }
        Ok((maj, val as usize))
    } else {
        Err("invalid header cbor_read_header_buf".into())
    }
}

/// Given a CBOR serialized IPLD buffer, read through all of it and return all the Links.
/// This function is useful because it is quite a bit more fast than doing this recursively on a
/// deserialized IPLD object.
fn scan_for_links<B: Read + Seek, F>(buf: &mut B, mut callback: F) -> Result<(), Box<dyn StdError>>
where
    F: FnMut(Cid) -> Result<(), Box<dyn StdError>>,
{
    let mut scratch: [u8; 100] = [0; 100];
    let mut remaining = 1;
    while remaining > 0 {
        let (maj, extra) = cbor_read_header_buf(buf, &mut scratch)?;
        match maj {
            // MajUnsignedInt, MajNegativeInt, MajOther
            0 | 1 | 7 => {}
            // MajByteString, MajTextString
            2 | 3 => {
                buf.seek(std::io::SeekFrom::Current(extra as i64))?;
            }
            // MajTag
            6 => {
                // Check if the tag refers to a CID
                if extra == 42 {
                    let (maj, extra) = cbor_read_header_buf(buf, &mut scratch)?;
                    // The actual CID is expected to be a byte string
                    if maj != 2 {
                        return Err("expected cbor type byte string in input".into());
                    }
                    if extra > 100 {
                        return Err("string in cbor input too long".into());
                    }
                    buf.read_exact(&mut scratch[..extra])?;
                    let c = Cid::try_from(&scratch[1..extra])?;
                    callback(c)?;
                } else {
                    remaining += 1;
                }
            }
            // MajArray
            4 => {
                remaining += extra;
            }
            // MajMap
            5 => {
                remaining += extra * 2;
            }
            _ => {
                return Err(format!("unhandled cbor type: {}", maj).into());
            }
        }
        remaining -= 1;
    }
    Ok(())
}

/// Copies the IPLD DAG under `root` from the cache to the base store.
fn copy_rec<BS>(
    base: &BS,
    cache: &HashMap<Cid, Vec<u8>>,
    root: Cid,
    buffer: &mut Vec<(Vec<u8>, Vec<u8>)>,
) -> Result<(), Box<dyn StdError>>
where
    BS: BlockStore,
{
    // Skip identity and Filecoin commitment Cids
    if root.codec() != DAG_CBOR {
        return Ok(());
    }
    let block = cache
        .get(&root)
        .ok_or_else(|| format!("Invalid link ({}) in flushing buffered store", root))?;

    scan_for_links(&mut Cursor::new(block), |link| {
        if link.codec() != DAG_CBOR {
            return Ok(());
        }
        // DB reads are expensive. So we check if it exists in the cache.
        // If it doesnt exist in the DB, which is likely, we proceed with using the cache.
        if !cache.contains_key(&link) {
            return Ok(());
        }
        // Recursively find more links under the links we're iterating over.
        copy_rec(base, cache, link, buffer)?;

        Ok(())
    })?;

    buffer.push((root.to_bytes(), block.clone()));

    Ok(())
}

impl<BS> BlockStore for BufferedBlockStore<'_, BS>
where
    BS: BlockStore,
{
    fn get_bytes(&self, cid: &Cid) -> Result<Option<Vec<u8>>, Box<dyn StdError>> {
        if let Some(data) = self.write.borrow().get(cid) {
            return Ok(Some(data.clone()));
        }

        self.base.get_bytes(cid)
    }

    fn put_raw(&self, bytes: Vec<u8>, code: Code) -> Result<Cid, Box<dyn StdError>> {
        let cid = cid::new_from_cbor(&bytes, code);
        self.write.borrow_mut().insert(cid, bytes);
        Ok(cid)
    }
}

impl<BS> Store for BufferedBlockStore<'_, BS>
where
    BS: Store,
{
    fn read<K>(&self, key: K) -> Result<Option<Vec<u8>>, Error>
    where
        K: AsRef<[u8]>,
    {
        self.base.read(key)
    }
    fn write<K, V>(&self, key: K, value: V) -> Result<(), Error>
    where
        K: AsRef<[u8]>,
        V: AsRef<[u8]>,
    {
        self.base.write(key, value)
    }
    fn delete<K>(&self, key: K) -> Result<(), Error>
    where
        K: AsRef<[u8]>,
    {
        self.base.delete(key)
    }
    fn exists<K>(&self, key: K) -> Result<bool, Error>
    where
        K: AsRef<[u8]>,
    {
        self.base.exists(key)
    }
    fn bulk_read<K>(&self, keys: &[K]) -> Result<Vec<Option<Vec<u8>>>, Error>
    where
        K: AsRef<[u8]>,
    {
        self.base.bulk_read(keys)
    }
    fn bulk_write<K, V>(&self, values: &[(K, V)]) -> Result<(), Error>
    where
        K: AsRef<[u8]>,
        V: AsRef<[u8]>,
    {
        self.base.bulk_write(values)
    }
    fn bulk_delete<K>(&self, keys: &[K]) -> Result<(), Error>
    where
        K: AsRef<[u8]>,
    {
        self.base.bulk_delete(keys)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cid::{multihash::MultihashDigest, Code, RAW};
    use commcid::commitment_to_cid;
    use forest_ipld::{ipld, Ipld};

    #[test]
    fn basic_buffered_store() {
        let mem = db::MemoryDB::default();
        let mut buf_store = BufferedBlockStore::new(&mem);

        let cid = buf_store.put(&8, Code::Blake2b256).unwrap();
        assert_eq!(mem.get::<u8>(&cid).unwrap(), None);
        assert_eq!(buf_store.get::<u8>(&cid).unwrap(), Some(8));

        buf_store.flush(&cid).unwrap();
        assert_eq!(buf_store.get::<u8>(&cid).unwrap(), Some(8));
        assert_eq!(mem.get::<u8>(&cid).unwrap(), Some(8));
        assert_eq!(buf_store.write.borrow().get(&cid), None);
    }

    #[test]
    fn buffered_store_with_links() {
        let mem = db::MemoryDB::default();
        let mut buf_store = BufferedBlockStore::new(&mem);
        let str_val = "value";
        let value = 8u8;
        let arr_cid = buf_store.put(&(str_val, value), Code::Blake2b256).unwrap();
        let identity_cid = Cid::new_v1(RAW, Code::Identity.digest(&[0u8]));

        // Create map to insert into store
        let sealed_comm_cid = commitment_to_cid(
            cid::FIL_COMMITMENT_SEALED,
            cid::POSEIDON_BLS12_381_A1_FC1,
            &[7u8; 32],
        )
        .unwrap();
        let unsealed_comm_cid = commitment_to_cid(
            cid::FIL_COMMITMENT_UNSEALED,
            cid::SHA2_256_TRUNC254_PADDED,
            &[5u8; 32],
        )
        .unwrap();
        let map = ipld!({
            "array": Link(arr_cid),
            "sealed": Link(sealed_comm_cid),
            "unsealed": Link(unsealed_comm_cid),
            "identity": Link(identity_cid),
            "value": str_val,
        });
        let map_cid = buf_store.put(&map, Code::Blake2b256).unwrap();

        let root_cid = buf_store.put(&(map_cid, 1u8), Code::Blake2b256).unwrap();

        // Make sure a block not connected to the root does not get written
        let unconnected = buf_store.put(&27u8, Code::Blake2b256).unwrap();

        assert_eq!(mem.get::<Ipld>(&map_cid).unwrap(), None);
        assert_eq!(mem.get::<Ipld>(&root_cid).unwrap(), None);
        assert_eq!(mem.get::<(String, u8)>(&arr_cid).unwrap(), None);
        assert_eq!(buf_store.get::<u8>(&unconnected).unwrap(), Some(27u8));

        // Flush and assert changes
        buf_store.flush(&root_cid).unwrap();
        assert_eq!(
            mem.get::<(String, u8)>(&arr_cid).unwrap(),
            Some((str_val.to_owned(), value))
        );
        assert_eq!(mem.get::<Ipld>(&map_cid).unwrap(), Some(map));
        assert_eq!(
            mem.get::<Ipld>(&root_cid).unwrap(),
            Some(ipld!([Link(map_cid), 1]))
        );
        assert_eq!(buf_store.get::<u8>(&identity_cid).unwrap(), None);
        assert_eq!(buf_store.get::<Ipld>(&unsealed_comm_cid).unwrap(), None);
        assert_eq!(buf_store.get::<Ipld>(&sealed_comm_cid).unwrap(), None);
        assert_eq!(mem.get::<u8>(&unconnected).unwrap(), None);
        assert_eq!(buf_store.get::<u8>(&unconnected).unwrap(), None);
    }
}
