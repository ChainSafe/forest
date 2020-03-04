use cid::Cid;
use leb128;
use unsigned_varint;
use std::fs::File;
use std::io::{BufReader, Read};
use forest_encoding::{ser::Serializer, de::Deserializer, from_slice};
use serde::{Serialize, Deserialize};

mod util;
use util::ld_read;

fn ls() -> std::io::Result<()> {
    // read file into stream
    // new car reader
    // for carreader.next() until EOF
    let mut file = File::open("devnet.car")?;
    let mut buf_reader = BufReader::new(file);
    let car_reader = CarReader::new(buf_reader);

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
where R: std::io::Read{
    pub fn new(mut buf_reader: BufReader<R>) -> Self {
       let (l, buf) = ld_read(&mut buf_reader);
        let header: CarHeader = from_slice(&buf).unwrap();
        CarReader {
            buf_reader,
            header,
        }
    }
    pub fn next (&self) -> Block {
//        self.buf_reader
        Block{}
    }

}
struct Block{}

#[cfg(test)]
mod tests {
    use super::*;
    use unsigned_varint;
    #[test]
    fn t1() {
        ls().unwrap();
    }

    #[test]
    fn t2() {
        let mut file = File::open("devnet.car").unwrap();
        let mut buf_reader = BufReader::new(file);
        let mut car_reader = CarReader::new(buf_reader);

        let (c, b) = util::read_node(&mut car_reader.buf_reader);

        println!("CID: {:?}, len: {}", c, b.len());

    }
}
