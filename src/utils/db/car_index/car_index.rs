use super::{BlockPosition, Hash, KeyValuePair, Slot};
use smallvec::{smallvec, SmallVec};
use std::io::{Error, ErrorKind, Read, Result, Seek, SeekFrom};

pub struct CarIndex<ReaderT> {
    reader: ReaderT,
    offset: u64,
    len: u64, // length of table in elements. Each element is 128bit.
}

impl<ReaderT: Read + Seek> CarIndex<ReaderT> {
    pub fn new(reader: ReaderT, offset: u64, len: u64) -> Self {
        CarIndex {
            reader,
            offset,
            len,
        }
    }

    fn slots(&mut self, mut index: u64) -> Result<impl Iterator<Item = Result<Slot>> + '_> {
        if index >= self.len {
            return Err(Error::new(ErrorKind::InvalidInput, "out-of-bound index"));
        }
        let len = self.len;
        self.reader
            .seek(SeekFrom::Start(self.offset + index * Slot::SIZE as u64))?;
        Ok(std::iter::from_fn(move || {
            if index == self.len {
                if let Err(err) = self.reader.seek(SeekFrom::Start(self.offset)) {
                    return Some(Err(err));
                }
                index = 0;
            }
            index += 1;
            Some(Slot::read(&mut self.reader))
        })
        .take(len as usize))
    }

    fn entries(&mut self, index: u64) -> Result<impl Iterator<Item = Result<KeyValuePair>> + '_> {
        Ok(self.slots(index)?.filter_map(|result| {
            result
                .map(|entry| match entry {
                    Slot::Empty => None,
                    Slot::Full(entry) => Some(entry),
                })
                .transpose()
        }))
    }

    pub fn lookup(&mut self, hash: Hash) -> Result<SmallVec<[BlockPosition; 1]>> {
        let len = self.len;
        let key = hash.optimal_offset(len as usize) as u64;
        self.reader
            .seek(SeekFrom::Start(self.offset + key * Slot::SIZE as u64))?;
        match Slot::read(&mut self.reader)? {
            Slot::Empty => Ok(smallvec![]),
            Slot::Full(first_entry) => {
                let mut smallest_dist = first_entry.hash.distance(key as usize, len as usize);
                self.entries(key)?
                    .take_while(move |result| match result {
                        Err(_) => true,
                        Ok(entry) => {
                            let hash_dist = entry.hash.distance(key as usize, len as usize);
                            smallest_dist = smallest_dist.min(hash_dist);
                            hash_dist == smallest_dist
                        }
                    })
                    .filter_map(move |result| {
                        result
                            .map(|entry| {
                                if hash == entry.hash {
                                    Some(entry.value)
                                } else {
                                    None
                                }
                            })
                            .transpose()
                    })
                    .collect::<Result<SmallVec<_>>>()
            }
        }
    }
}
