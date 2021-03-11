// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#![cfg(feature = "buffered")]

use super::BlockStore;
use cid::{Cid, Code, DAG_CBOR};
use db::{Error, Store};
use encoding::{StreamDeserializer, from_slice, to_vec};
use forest_ipld::Ipld;
use std::{borrow::Borrow, cell::{RefCell, RefMut}, convert::TryFrom, fs::File, io::{Cursor, Read}, time::SystemTime};
use std::collections::{BTreeMap,HashMap};
use std::error::Error as StdError;
use byteorder::{BigEndian, ByteOrder, ReadBytesExt};
use std::io::Seek;

/// Wrapper around `BlockStore` to limit and have control over when values are written.
/// This type is not threadsafe and can only be used in synchronous contexts.
#[derive(Debug)]
pub struct BufferedBlockStore<'bs, BS> {
    base: &'bs BS,
    write: RefCell<HashMap<Cid, Vec<u8>>>,
    buffer1: RefCell<HashMap<Vec<u8>, Vec<u8>>>,
    buffer2: RefCell<HashMap<Vec<u8>, Vec<u8>>>,

}

impl<'bs, BS> BufferedBlockStore<'bs, BS>
where
    BS: BlockStore,
{
    pub fn new(base: &'bs BS) -> Self {
        Self {
            base,
            write: Default::default(),
            buffer1: Default::default(),
            buffer2: Default::default(),
        }
    }
    /// Flushes the buffered cache based on the root node.
    /// This will recursively traverse the cache and write all data connected by links to this
    /// root Cid.
    pub fn flush(&mut self, root: &Cid) -> Result<(), Box<dyn StdError>> {
        // let guard = pprof::ProfilerGuard::new(100).unwrap();
        let now = SystemTime::now();
        println!("--------------- The cache has: {} items -----------------", self.write.borrow().len());


        println!("REC 2 START: {:?}", now.elapsed());
        match copy_rec(self.base, &self.write.borrow(), &mut self.buffer1.borrow_mut(), *root) {
            Ok(_) => {}
            Err(e) => {println!("REC 2 FAILED!: {}", e);}
        }
         println!("----------------- WE 2 GOT: {:?} items --------------------", self.buffer1.borrow().len());
 
 
 

        // println!("REC 1 START: {:?}", now.elapsed());
        // write_recursive(self.base, &self.write.borrow(), &mut self.buffer1.borrow_mut(), root)?;
        // println!("----------------- WE 1 GOT: {:?} items --------------------", self.buffer1.borrow().len());



        println!("------- Start DB write START: {:?}", now.elapsed());
        // let mut counter = 0;
        // for (raw_cid_bz, raw_bz) in self.buffer1.borrow().iter() {
        //     counter += 1;
        //     self.base.write(raw_cid_bz, raw_bz)?;
        // }
        self.write = Default::default();

        // match guard.report().build() {
        //     Ok(report) => {
        //         let file = File::create("flamegraph.svg").unwrap();
        //         report.flamegraph(file).unwrap();
    
        //         // println!("report: {:?}", &report);
        //     }
        //     Err(_) => {}
        // };
        // println!("------- Ended up writing: {} iterms", counter);

        println!("--------------- FLUSH TOTAL TOOK: {:?} ---------------------------------", now.elapsed());
        Ok(())
    }
}

fn cbor_read_header_buf<'a, B: std::io::BufRead>(br: &mut B, scratch: &'a mut [u8]) -> Result<(u8, u64), Box<dyn StdError>> 
{
    let first = br.read_u8()?;
    let maj = (first & 0xe0) >> 5;
    let low = first & 0x1f;

    if low < 24 {
        return Ok((maj, low as u64));
    } else if low == 24 {
        let next = br.read_u8()?;
        if next < 24 {
            return Err(format!("cbor input was not canonical (lval 24 with value < 24)").into());
        }
        return Ok((maj, next as u64));
    } else if low == 25 {
        br.read_exact(&mut scratch[..2])?;
        let val = BigEndian::read_u16(&scratch[..2]);
        if val <= u8::MAX as u16 {
            return Err(format!("cbor input was not canonical (lval 25 with value <= MaxUint8)").into());
        }
        return Ok((maj, val as u64))
    } else if low == 26 {
        br.read_exact(&mut scratch[..4])?;
        let val = BigEndian::read_u32(&scratch[..4]);
        if val <= u16::MAX as u32 {
            return Err(format!("cbor input was not canonical (lval 26 with value <= MaxUint16)").into());
        }
        return Ok((maj, val as u64))

    } else if low == 27 {
        br.read_exact(&mut scratch[..8])?;
        let val = BigEndian::read_u64(&scratch[..8]);
        if val <= u32::MAX as u64 {
            return Err(format!("cbor input was not canonical (lval 27 with value <= MaxUint32)").into());
        }
        return Ok((maj, val))

    } else {
        return Err(format!("invalid header cbor_read_header_buf").into());
    }
}

fn new_scan_for_linkz<B: std::io::BufRead + Seek>(br: &mut B) -> Result<Vec<Cid>, Box<dyn StdError>> {
    let mut scratch : [u8; 100] = [0;100];
    let mut remaining = 1;
    let mut ret = Vec::new();
    while remaining > 0 {
        let (maj, extra) = cbor_read_header_buf(br, &mut scratch)?;
        match maj {
            0 | 1 | 7 => {},
            2 | 3 => {
                br.seek(std::io::SeekFrom::Current(extra as i64))?;
            }
            6 => {
                if extra == 42 {
                    let (maj, extra) = cbor_read_header_buf(br, &mut scratch)?;
                    if maj != 2 {
                        return Err(format!("expected cbor type byte string in input").into());
                    }
                    if extra > 100 {
                        return Err(format!("string in cbor input too long").into());
                    }
                    br.read_exact(&mut scratch[..extra as usize])?;
                    let c = Cid::try_from(&scratch[1..extra as usize])?;
                    ret.push(c);
                } else {
                    remaining += 1;
                }
            }
            4 => {
                remaining += extra;
            }
            5 => {
                remaining += extra * 2;
            }
            _ => {
                return Err(format!("unhandled cbor type: {}", maj).into());
            }
        }
        remaining -= 1;
    }
    Ok(ret)
}

fn copy_rec<BS>(
    base: &BS,
    cache: &HashMap<Cid, Vec<u8>>,
    buffer: &mut RefMut<HashMap<Vec<u8>, Vec<u8>>>,

    root: Cid,
    // cb: &mut F,
) -> Result<(), Box<dyn StdError>> 
where 
// F: FnMut(&Vec<u8>, &Vec<u8>) -> Result<(), Box<dyn StdError>>,
BS: BlockStore,
{
    if root.codec() != DAG_CBOR {
        return Ok(());
    }
    let block = cache.get(&root).ok_or_else(|| format!("Invalid link ({}) in flushing buffered store", root))?;
    let links = new_scan_for_linkz(&mut std::io::BufReader::new(Cursor::new(block)))?;
    // println!("Got {} links!", links.len());

    for link in links.iter() {
        if link.codec() != DAG_CBOR {
            continue;
        }
        // if base.exists(link.to_bytes())? {
        //     continue;
        // }
        // DB reads are expensive. Often times, if the cache doesnt have the key, then the DB will have it. 
        // And if the DB doesnt have it, the cache will.
        if !cache.contains_key(&link) && base.exists(link.to_bytes())? {
            continue;
        }
        copy_rec(base,cache,buffer, *link)?;
    }
    base.write(&root.to_bytes(), block)?;
    // cb(&root.to_bytes(), block)?;
    // buffer.insert(root.to_bytes(), block.to_vec());
    Ok(())
}

// fn scan_for_links(r: &[u8]) -> Result<Vec<Cid>, Box<dyn StdError>>
// {
//     let mut br = encoding::Deserializer::from_slice(&r).into_iter::<encoding::value::Value>();

//     let mut ret = Vec::new();
//         // println!("majjj: {:?}", maj);
//         while let Some (maj) = br.next() {
//             match maj {
//             Ok(v) => {
//                 match v {
//                     encoding::value::Value::Null | encoding::value::Value::Bool(_) | encoding::value::Value::Float(_) | encoding::value::Value::Integer(_) => {},
//                     encoding::value::Value::Text(_) | encoding::value::Value::Bytes(_)  => {}
//                     encoding::value::Value::Array(v) => {
//                         // let mut k: Vec<_> = v.par_iter().map(|val| {
//                         //     scan_for_links(&to_vec(&val).unwrap()).unwrap()
//                         // }).flatten().collect();
//                         // // println!("wow we got k: {}", k.len());
//                         // ret.append(&mut k);
//                         for val in v.iter() {
//                             ret.append(&mut scan_for_links(&to_vec(&val)?)?);
//                         }
//                     },
//                     encoding::value::Value::Map(v) => {
//                         for val in v.values() {
//                             ret.append(&mut scan_for_links(&to_vec(&val)?)?);
//                         }
//                     },
//                     encoding::value::Value::Tag(tag, value) => {
//                             if let encoding::value::Value::Bytes(b) = value.borrow() {
//                                 let c: Cid = Cid::try_from(&b[1..])?;
//                                 ret.push(c);
//                             }
//                     },
//                     _ => {
//                         // println!("finally hit unrech");
//                         unreachable!("0")
//                         // throw error her
//                     }
//                 }
//             },
//             Err(e) => {
//                 // println!("finally hit err {}", e.to_string());
//                 unreachable!("1")
//             }
//         }
//     }
        
    

//     Ok(ret)
// }

/// Recursively traverses cache through Cid links.
fn write_recursive<BS>(
    base: &BS,
    cache: &HashMap<Cid, Vec<u8>>,
    buffer: &mut RefMut<HashMap<Vec<u8>, Vec<u8>>>,
    cid: &Cid,
) -> Result<(), Box<dyn StdError>>
where
    BS: BlockStore,
{
    // Skip identity and Filecoin commitment Cids
    if cid.codec() != DAG_CBOR {
        return Ok(());
    }

    let raw_cid_bz = cid.to_bytes();

    // If root exists in base store already, can skip
    // DB reads are expensive. Often times, if the cache doesnt have the key, then the DB will have it. 
    // And if the DB doesnt have it, the cache will.
    if base.exists(cid.to_bytes())? {
        return Ok(());
    }

    let raw_bz = cache
        .get(cid)
        .ok_or_else(|| format!("Invalid link ({}) in flushing buffered store", cid))?;

    // Deserialize the bytes to Ipld to traverse links.
    // This is safer than finding links in place,
    // but slightly slower to copy and potentially allocate non Cid data.
    let block = from_slice(raw_bz)?;

    // Traverse and write linked data recursively
    let links = for_each_link(&block, &mut |c| write_recursive(base, cache, buffer, c))?;
    for link in links {
        write_recursive(base, cache, buffer, &link)?;
    }

    // Write the root node to base storage
    // base.write(&raw_cid_bz, raw_bz)?;
    buffer.insert(raw_cid_bz, raw_bz.to_vec());
    Ok(())
}

/// Recursively explores Ipld for links and calls a function with a reference to the Cid.
fn for_each_link<F>(ipld: &Ipld, cb: &mut F) -> Result<Vec<Cid>, Box<dyn StdError>>
where
    F: FnMut(&Cid) -> Result<(), Box<dyn StdError>>,
{
    let mut ret = Vec::new();
    match ipld {
        Ipld::Link(c) => ret.push(*c),
        Ipld::List(arr) => {
            for item in arr {
                ret.append(&mut for_each_link(item, cb)?);
            }
        }
        Ipld::Map(map) => {
            for v in map.values() {
                ret.append(&mut for_each_link(v, cb)?);
            }
        }
        _ => (),
    }
    Ok(ret)
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
            "array": Link(arr_cid.clone()),
            "sealed": Link(sealed_comm_cid.clone()),
            "unsealed": Link(unsealed_comm_cid.clone()),
            "identity": Link(identity_cid.clone()),
            "value": str_val,
        });
        let map_cid = buf_store.put(&map, Code::Blake2b256).unwrap();

        let root_cid = buf_store
            .put(&(map_cid.clone(), 1u8), Code::Blake2b256)
            .unwrap();

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
