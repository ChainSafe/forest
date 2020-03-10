// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod error;
mod util;

use blockstore::BlockStore;
use cid::Cid;
use error::*;
use forest_encoding::from_slice;
use serde::{Deserialize, Serialize};
use std::io::{BufReader, Read};
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
    pub buf_reader: BufReader<R>,
    pub header: CarHeader,
}

impl<R> CarReader<R>
where
    R: Read,
{
    /// Creates a new CarReader and parses the CarHeader
    pub fn new(mut buf_reader: BufReader<R>) -> Result<Self, Error> {
        let buf = ld_read(&mut buf_reader)?;
        let header: CarHeader = from_slice(&buf).map_err(|e| Error::ParsingError(e.to_string()))?;
        if header.roots.is_empty() {
            return Err(Error::ParsingError("empty CAR file".to_owned()));
        }
        if header.version != 1 {
            return Err(Error::InvalidFile("CAR file version must be 1".to_owned()));
        }
        Ok(CarReader { buf_reader, header })
    }

    /// Returns the next IPLD Block in the buffer
    pub fn next_block(&mut self) -> Result<Block, Error> {
        // Read node -> cid, bytes
        let (cid, data) = read_node(&mut self.buf_reader)?;
        Ok(Block { cid, data })
    }
}

/// IPLD Block
#[derive(Clone, Debug)]
pub struct Block {
    cid: Cid,
    data: Vec<u8>,
}

/// Loads a CAR buffer into a BlockStore
pub fn load_car<R: Read, B: BlockStore>(s: &B, buf_reader: BufReader<R>) -> Result<(), Error> {
    let mut car_reader = CarReader::new(buf_reader)?;

    while !car_reader.buf_reader.buffer().is_empty() {
        let block = car_reader.next_block()?;
        s.write(block.cid.to_bytes(), block.data)
            .map_err(|e| Error::Other(e.to_string()))?;
    }
    Ok(())
}
