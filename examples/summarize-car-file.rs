//! Uses the reference decoders, to sense-check custom decoders

use clap::Parser;
use fvm_ipld_car::CarReader;
use std::path::PathBuf;
use tokio::{fs::File, io::BufReader};
use tokio_util::compat::TokioAsyncReadCompatExt as _;

#[derive(Parser)]
struct Args {
    path: PathBuf,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let Args { path } = Args::parse();
    let mut reader = CarReader::new(BufReader::new(File::open(path).await?).compat()).await?;

    let mut count = 0;
    let mut min = usize::MAX;
    let mut max = usize::MIN;
    while let Some(block) = reader.next_block().await? {
        count += 1;
        let len = block.data.len();
        if len > max {
            max = len
        }
        if len < min {
            min = len
        }
    }
    println!(
        "{count} blocks, min {}, max {}",
        human_bytes(min),
        human_bytes(max)
    );

    Ok(())
}

fn human_bytes(bytes: impl Into<byte_unit::Byte>) -> String {
    bytes.into().get_appropriate_unit(true).format(2)
}
