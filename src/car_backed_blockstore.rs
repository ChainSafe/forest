use ahash::AHashMap;
use cid::Cid;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_car::CarHeader;
use integer_encoding::VarIntReader as _;
use parking_lot::Mutex;
use std::collections::hash_map::Entry::{Occupied, Vacant};
use std::{
    io::{
        self, BufRead,
        ErrorKind::{InvalidData, UnexpectedEof, Unsupported},
        Read, Seek, SeekFrom,
    },
    sync::Arc,
};
use tracing::debug;

/// If you seek to `offset` (from the start of the file), and read `length` bytes,
/// you should get `cid` and its corresponding data.
///
/// ```text
///          ├────────┤
///          │ length │
///        │ ├────────┤◄─block offset
///        │ │ CID    │
///        │ ├────────┤
///  block │ │ data.. │
/// length │ │        │
///        ▼ ├────────┤
/// ```
#[derive(Debug)]
struct BlockLocation {
    offset: u64,
    length: usize,
}

// Theoretically, this should be clonable, with very low overhead
pub struct CarBackedBlockstore<ReaderT> {
    // go is a gc language, you say?
    reader: Mutex<ReaderT>,
    index: AHashMap<Cid, BlockLocation>,
    roots: Vec<Cid>,
    // go is a gc language, you say?
    write_cache: Mutex<AHashMap<Cid, Vec<u8>>>,
}

impl<ReaderT> CarBackedBlockstore<ReaderT>
where
    ReaderT: BufRead + Seek,
{
    // Reader should read immutable data (should be flocked)
    // Buffer should have room for a car header
    // Makes blocking calls
    #[tracing::instrument(skip_all)]
    pub fn new(mut reader: ReaderT) -> cid::Result<Self> {
        let CarHeader { roots, version } = read_header(&mut reader)?;
        if version != 1 {
            return Err(cid::Error::Io(io::Error::new(
                Unsupported,
                "file must be CARv1",
            )));
        }

        // now create the index
        let index = std::iter::from_fn(|| read_block(&mut reader))
            .collect::<Result<AHashMap<_, _>, _>>()?;
        if index.is_empty() {
            return Err(cid::Error::Io(io::Error::new(
                InvalidData,
                "CARv1 files must not be empty",
            )));
        }

        Ok(Self {
            reader: Mutex::new(reader),
            index,
            roots,
            write_cache: Mutex::new(AHashMap::new()),
        })
    }
}

impl<ReaderT> Blockstore for CarBackedBlockstore<ReaderT>
where
    ReaderT: Read + Seek,
{
    // This function should return a Cow<[u8]> at the very least
    #[tracing::instrument(skip(self))]
    fn get(&self, k: &Cid) -> anyhow::Result<Option<Vec<u8>>> {
        match (self.index.get(k), self.write_cache.lock().entry(*k)) {
            (Some(_location), Occupied(cached)) => {
                debug!("evicting from write cache");
                Ok(Some(cached.remove()))
            }
            (Some(BlockLocation { offset, length }), Vacant(_)) => {
                debug!("fetching from disk");
                let mut reader = self.reader.lock();
                reader.seek(SeekFrom::Start(*offset))?;
                let cid = Cid::read_bytes(&mut *reader)?;
                assert_eq!(cid, *k);
                let cid_len = reader.stream_position()? - *offset;
                let data_len = *length - usize::try_from(cid_len).unwrap();
                let mut data = vec![0; data_len];
                reader.read_exact(&mut data)?;
                Ok(Some(data))
            }
            (None, Occupied(cached)) => {
                debug!("getting from write cache");
                Ok(Some(cached.get().clone()))
            }
            (None, Vacant(_)) => {
                debug!("not found");
                Ok(None)
            }
        }
    }

    #[tracing::instrument(skip(self, block))]
    fn put_keyed(&self, k: &Cid, block: &[u8]) -> anyhow::Result<()> {
        match self.write_cache.lock().entry(*k) {
            Occupied(already) if already.get() == block => {
                debug!("already in cache");
                Ok(())
            }
            Occupied(_) => panic!("mismatched data for cid {}", k),
            Vacant(vacant) => {
                debug!("insert into cache");
                vacant.insert(block.to_owned());
                Ok(())
            }
        }
    }
}

fn read_header(reader: &mut impl BufRead) -> io::Result<CarHeader> {
    let header_len = reader.read_varint::<usize>()?;
    match reader.fill_buf()? {
        buf if buf.is_empty() => Err(io::Error::from(UnexpectedEof)),
        nonempty if nonempty.len() < header_len => Err(io::Error::new(
            UnexpectedEof,
            "header is too short, or BufReader doesn't have enough capacity for a header",
        )),
        header_etc => match fvm_ipld_encoding::from_slice(&header_etc[..header_len]) {
            Ok(header) => {
                reader.consume(header_len);
                Ok(header)
            }
            Err(e) => Err(io::Error::new(InvalidData, e)),
        },
    }
}

// importantly, we seek _past_ the data
fn read_block(reader: &mut (impl BufRead + Seek)) -> Option<cid::Result<(Cid, BlockLocation)>> {
    match reader.fill_buf() {
        Ok(buf) if buf.is_empty() => None, // EOF
        Ok(_nonempty) => match (
            reader.read_varint::<usize>(),
            reader.stream_position(),
            Cid::read_bytes(&mut *reader),
        ) {
            (Ok(length), Ok(offset), Ok(cid)) => {
                let next_block_offset = offset + u64::try_from(length).unwrap();
                if let Err(e) = reader.seek(SeekFrom::Start(next_block_offset)) {
                    return Some(Err(cid::Error::Io(e)));
                }
                Some(Ok((cid, BlockLocation { offset, length })))
            }
            (Err(e), _, _) | (_, Err(e), _) => Some(Err(cid::Error::Io(e))),
            (_, _, Err(e)) => Some(Err(e)),
        },
        Err(e) => Some(Err(cid::Error::Io(e))),
    }
}

#[cfg(test)]
mod tests {
    use super::CarBackedBlockstore;
    use futures_util::AsyncRead;
    use fvm_ipld_blockstore::{Blockstore as _, MemoryBlockstore};
    use fvm_ipld_car::{Block, CarReader};

    #[test]
    fn test() {
        let car = include_bytes!("../test-snapshots/chain4.car");
        let reference = reference(futures::io::Cursor::new(car));
        let car_backed = CarBackedBlockstore::new(std::io::Cursor::new(car)).unwrap();

        assert_eq!(car_backed.index.len(), 1222);
        assert_eq!(car_backed.roots.len(), 1);

        for cid in car_backed.index.keys() {
            let expected = reference.get(cid).unwrap().unwrap();
            let actual = car_backed.get(cid).unwrap().unwrap();
            assert_eq!(expected, actual);
        }
    }

    fn reference(reader: impl AsyncRead + Send + Unpin) -> MemoryBlockstore {
        futures::executor::block_on(async {
            let blockstore = MemoryBlockstore::new();
            let mut blocks = CarReader::new(reader).await.unwrap();
            while let Some(Block { cid, data }) = blocks.next_block().await.unwrap() {
                blockstore.put_keyed(&cid, &data).unwrap()
            }
            blockstore
        })
    }
}
