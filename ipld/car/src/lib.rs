// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod error;
mod util;

use blockstore::BlockStore;
use cid::Cid;
use error::*;
use forest_encoding::from_slice;
use serde::{Deserialize, Serialize};
use std::io::Read;
use util::{ld_read, read_node};

/// CAR file header
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct CarHeader {
    pub roots: Vec<Cid>,
    pub version: u64,
}

impl CarHeader {
    /// Creates a new CAR file header
    pub fn new(roots: Vec<Cid>, version: u64) -> Self {
        Self { roots, version }
    }
}

/// Reads CAR files that are in a BufReader
pub struct CarReader<R> {
    pub reader: R,
    pub header: CarHeader,
}

impl<R> CarReader<R>
where
    R: Read,
{
    /// Creates a new CarReader and parses the CarHeader
    pub fn new(mut reader: R) -> Result<Self, Error> {
        let buf = ld_read(&mut reader)?
            .ok_or_else(|| Error::ParsingError("failed to parse uvarint for header".to_string()))?;
        let header: CarHeader = from_slice(&buf).map_err(|e| Error::ParsingError(e.to_string()))?;
        if header.roots.is_empty() {
            return Err(Error::ParsingError("empty CAR file".to_owned()));
        }
        if header.version != 1 {
            return Err(Error::InvalidFile("CAR file version must be 1".to_owned()));
        }
        Ok(CarReader { reader, header })
    }

    /// Returns the next IPLD Block in the buffer
    pub fn next_block(&mut self) -> Result<Option<Block>, Error> {
        // Read node -> cid, bytes
        let block = read_node(&mut self.reader)?.map(|(cid, data)| Block { cid, data });
        Ok(block)
    }
}

/// IPLD Block
#[derive(Clone, Debug)]
pub struct Block {
    cid: Cid,
    data: Vec<u8>,
}

/// Loads a CAR buffer into a BlockStore
pub fn load_car<R: Read, B: BlockStore>(s: &B, reader: R) -> Result<Vec<Cid>, Error> {
    let mut car_reader = CarReader::new(reader)?;

    // Batch write key value pairs from car file
    let mut buf: Vec<(Vec<u8>, Vec<u8>)> = Vec::with_capacity(100);
    while let Some(block) = car_reader.next_block()? {
        buf.push((block.cid.to_bytes(), block.data));
        if buf.len() > 1000 {
            s.bulk_write(&buf)
                .map_err(|e| Error::Other(e.to_string()))?;
            buf.clear();
        }
    }
    s.bulk_write(&buf)
        .map_err(|e| Error::Other(e.to_string()))?;
    Ok(car_reader.header.roots)
}
