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

use anyhow::Context;
use bytes::Bytes;
use clap::Parser;
use futures_util::{StreamExt, TryStreamExt};
use fvm_ipld_car::CarHeader;
use std::path::PathBuf;
use tokio::fs::File;
use tokio_util_06::codec::FramedRead;
use unsigned_varint::codec::UviBytes;

#[derive(Parser)]
struct Args {
    path: PathBuf,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let Args { path } = Args::parse();
    let mut frames = FramedRead::new(File::open(path).await?, UviBytes::<Bytes>::default());
    let header = frames
        .next()
        .await
        .context("no header")?
        .context("error getting header")?;
    let header = fvm_ipld_encoding::from_slice::<CarHeader>(&header).context("invalid header")?;
    println!("header has {} roots", header.roots.len());
    let frame_lengths = frames
        .map_ok(|frame| frame.len())
        .try_collect::<Vec<_>>()
        .await?;
    println!("{}", frame_lengths.len());
    Ok(())
}
