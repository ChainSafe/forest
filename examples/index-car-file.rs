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

use cid::Cid;
use clap::Parser;
use fvm_ipld_car::CarHeader;
use integer_encoding::VarIntReader as _;
use std::{
    fs::File,
    io::{self, BufRead, BufReader, Seek, SeekFrom},
    path::PathBuf,
};
use tracing::{info, info_span};
use tracing_subscriber::{filter::LevelFilter, fmt::format::FmtSpan, EnvFilter};

#[derive(Parser)]
struct Args {
    path: PathBuf,
}

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
struct BlockOffset {
    cid: Cid,
    length: usize,
    offset: u64,
}

fn read_header(reader: &mut impl BufRead) -> io::Result<CarHeader> {
    use std::io::{
        Error,
        ErrorKind::{InvalidData, UnexpectedEof},
    };
    let header_len = reader.read_varint::<usize>()?;
    match reader.fill_buf()? {
        buf if buf.is_empty() => Err(Error::from(UnexpectedEof)),
        nonempty if nonempty.len() < header_len => Err(Error::new(
            UnexpectedEof,
            "header is too short, or BufReader doesn't have enough capacity for a header",
        )),
        header_etc => match fvm_ipld_encoding::from_slice(&header_etc[..header_len]) {
            Ok(header) => {
                reader.consume(header_len);
                Ok(header)
            }
            Err(e) => Err(Error::new(InvalidData, e)),
        },
    }
}

fn read_block(reader: &mut (impl BufRead + Seek)) -> Option<cid::Result<BlockOffset>> {
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
                Some(Ok(BlockOffset {
                    cid,
                    length,
                    offset,
                }))
            }
            (Err(e), _, _) | (_, Err(e), _) => Some(Err(cid::Error::Io(e))),
            (_, _, Err(e)) => Some(Err(e)),
        },
        Err(e) => Some(Err(cid::Error::Io(e))),
    }
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::builder()
                .with_default_directive(LevelFilter::INFO.into())
                .from_env()?,
        )
        .with_file(false)
        .with_target(false)
        .with_span_events(FmtSpan::NEW | FmtSpan::CLOSE)
        .with_writer(std::io::stderr)
        .init();
    let Args { path } = Args::parse();

    // TODO(aatifsyed): lock the file
    let mut reader = BufReader::new(File::open(path)?);
    let file_len = reader.get_ref().metadata()?.len();

    let CarHeader { roots, version } = read_header(&mut reader)?;
    info!(version, num_roots = roots.len(), "header");

    let blocks = info_span!("create index").in_scope(|| {
        std::iter::from_fn(|| read_block(&mut reader)).collect::<Result<Vec<_>, _>>()
    })?;
    let index_len = std::mem::size_of_val(blocks.as_slice());
    info!(
        num_blocks = blocks.len(),
        index = human_bytes(index_len),
        file = human_bytes(file_len),
        "indexed"
    );

    info_span!("verify all offsets").in_scope(|| {
        for BlockOffset {
            cid: expected,
            length: _,
            offset,
        } in blocks.iter().rev()
        {
            reader.seek(SeekFrom::Start(*offset))?;
            let actual = Cid::read_bytes(&mut reader)?;
            assert_eq!(expected, &actual);
        }
        cid::Result::Ok(())
    })?;

    Ok(())
}

fn human_bytes(bytes: impl Into<byte_unit::Byte>) -> String {
    bytes.into().get_appropriate_unit(true).format(2)
}
