//! A car file looks like this
//! - header section
//!   - section length (varint)
//!   - header contents (cbor)
//! - body section
//!   - section length (varint)
//!   - cid
//!   - contents
//! - body section
//!   - ...
//! - ...

use clap::Parser;
use integer_encoding::VarIntReader as _;
use std::{
    fs::File,
    io::{self, BufRead, BufReader, Seek},
    path::PathBuf,
};

#[derive(Parser)]
struct Args {
    path: PathBuf,
}

struct Section {
    body_length: usize,
    /// From start of file
    body_offset: u64,
}

fn read_section(reader: &mut (impl BufRead + Seek)) -> Option<io::Result<Section>> {
    match reader.fill_buf() {
        Ok(buf) if buf.is_empty() => None, // EOF
        Ok(_nonempty) => match (reader.read_varint::<usize>(), reader.stream_position()) {
            (Ok(body_length), Ok(body_offset)) => {
                if let Err(e) = reader.seek(io::SeekFrom::Current(body_length.try_into().unwrap()))
                {
                    return Some(Err(e));
                }
                Some(Ok(Section {
                    body_length,
                    body_offset,
                }))
            }
            (Ok(_), Err(e)) | (Err(e), _) => Some(Err(e)),
        },
        Err(e) => Some(Err(e)),
    }
}

fn main() -> anyhow::Result<()> {
    let Args { path } = Args::parse();
    // TODO(aatifsyed): lock the file
    let mut reader = BufReader::new(File::open(path)?);

    let count = std::iter::from_fn(|| read_section(&mut reader)).collect::<Result<Vec<_>, _>>()?;
    println!("{}", count.len());
    Ok(())
}
