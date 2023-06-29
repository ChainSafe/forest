use ahash::AHashMap;
use anyhow::Context as _;
use cid::Cid;
use std::{
    fs::File,
    io::{
        self, BufReader,
        ErrorKind::{InvalidData, Other},
        Read, Seek, SeekFrom,
    },
    path::PathBuf,
};

#[derive(clap::Parser)]
struct Args {
    path: PathBuf,
    #[arg(short, long)]
    mode: Mode,
}

#[derive(clap::ValueEnum, Clone)]
enum Mode {
    Buffer8k,
    Buffer1k,
    Buffer100,
    Unbuffered,
}

fn main() -> anyhow::Result<()> {
    let Args { path, mode } = clap::Parser::parse();
    let mut file = File::open(path)?;

    let (_header_start, header_len) = next_varint_frame(&mut file)?.context("no header")?;
    file.seek(SeekFrom::Current(i64::from(header_len)))?;
    let index = match mode {
        Mode::Buffer100 => {
            let mut buffered = BufReader::with_capacity(100usize.next_power_of_two(), file);
            std::iter::from_fn(|| read_block_location_or_eof(&mut buffered).transpose())
                .collect::<Result<AHashMap<_, _>, _>>()?
        }
        Mode::Buffer1k => {
            let mut buffered = BufReader::with_capacity(1000usize.next_power_of_two(), file);
            std::iter::from_fn(|| read_block_location_or_eof(&mut buffered).transpose())
                .collect::<Result<AHashMap<_, _>, _>>()?
        }
        Mode::Buffer8k => {
            let mut buffered = BufReader::with_capacity(8000usize.next_power_of_two(), file);
            std::iter::from_fn(|| read_block_location_or_eof(&mut buffered).transpose())
                .collect::<Result<AHashMap<_, _>, _>>()?
        }
        Mode::Unbuffered => {
            std::iter::from_fn(|| read_block_location_or_eof(&mut file).transpose())
                .collect::<Result<AHashMap<_, _>, _>>()?
        }
    };

    println!("{}", index.len());

    Ok(())
}

/// Importantly, we seek _past_ the data, rather than read any in.
/// This allows us to keep indexing fast.
///
/// [`Ok(None)`] on EOF
fn read_block_location_or_eof(
    mut reader: (impl Read + Seek),
) -> cid::Result<Option<(Cid, (u64, u32))>> {
    let Some((frame_body_offset, body_length)) = next_varint_frame(&mut reader)? else {
        return Ok(None)
    };
    let cid = Cid::read_bytes(&mut reader)?;
    // tradeoff: we perform a second syscall here instead of in Blockstore::get,
    // and keep BlockDataLocation purely for the blockdata
    let block_data_offset = reader.stream_position()?;
    let next_frame_offset = frame_body_offset + u64::from(body_length);
    let block_data_length = u32::try_from(next_frame_offset - block_data_offset).unwrap();
    reader.seek(SeekFrom::Start(next_frame_offset))?;
    Ok(Some((cid, (block_data_offset, block_data_length))))
}

fn next_varint_frame(mut reader: (impl Read + Seek)) -> io::Result<Option<(u64, u32)>> {
    Ok(match read_u32_or_eof(&mut reader)? {
        Some(body_length) => {
            let frame_body_offset = reader.stream_position()?;
            Some((frame_body_offset, body_length))
        }
        None => None,
    })
}

fn read_u32_or_eof(mut reader: impl Read) -> io::Result<Option<u32>> {
    use unsigned_varint::io::{
        read_u32,
        ReadError::{Decode, Io},
    };

    let mut byte = [0u8; 1]; // detect EOF
    match reader.read(&mut byte)? {
        0 => Ok(None),
        1 => read_u32(byte.chain(reader))
            .map_err(|varint_error| match varint_error {
                Io(e) => e,
                Decode(e) => io::Error::new(InvalidData, e),
                other => io::Error::new(Other, other),
            })
            .map(Some),
        _ => unreachable!(),
    }
}
