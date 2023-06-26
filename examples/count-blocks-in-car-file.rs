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
    while let Some(_block) = reader.next_block().await? {
        count += 1;
    }
    println!("{count}");

    Ok(())
}
