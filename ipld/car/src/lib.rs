use cid::Cid;
use forest_encoding::{de::Deserializer, from_slice, ser::Serializer};
use leb128;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{BufReader, Read};
use unsigned_varint;

mod error;
mod util;
use crate::util::read_node;
use error::*;
use util::ld_read;

fn ls() -> Result<(), Error> {
    let mut file = File::open("devnet.car").unwrap();

    let mut buf_reader = BufReader::new(file);
    let mut car_reader = CarReader::new(buf_reader)?;

    // for carreader next
    while !car_reader.buf_reader.buffer().is_empty() {
        let x = car_reader.next()?;
        println!("CID{:?}, Data Len: {}", x.cid.to_string(), x.data.len());
    }
    Ok(())
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct CarHeader {
    pub roots: Vec<Cid>,
    pub version: u64,
}

impl CarHeader {
    pub fn new(roots: Vec<Cid>, version: u64) -> Self {
        Self { roots, version }
    }
}

struct CarReader<R> {
    buf_reader: BufReader<R>,
    header: CarHeader,
}

impl<R> CarReader<R>
where
    R: std::io::Read,
{
    pub fn new(mut buf_reader: BufReader<R>) -> Result<Self, Error> {
        let (len, buf) = ld_read(&mut buf_reader)?;
        let header: CarHeader = from_slice(&buf).map_err(|e| Error::ParsingError(e.to_string()))?;
        // TODO: Do some checks here
        Ok(CarReader { buf_reader, header })
    }
    pub fn next(&mut self) -> Result<Block, Error> {
        // Read node -> cid, bytes
        let (cid, data) = read_node(&mut self.buf_reader)?;
        let _ = cid.prefix();

        Ok(Block { cid, data })
    }
}
struct Block {
    cid: Cid,
    data: Vec<u8>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use unsigned_varint;
    #[test]
    fn t1() {
        ls().unwrap();
    }
}
