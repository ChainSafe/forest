use bytes::Buf as _;
use futures_util::{future::OptionFuture, StreamExt as _, TryStreamExt as _};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use std::{cell::RefCell, path::PathBuf};
use tokio::{fs::File, io::AsyncWriteExt};
use tokio_util_06::codec::{FramedRead, FramedWrite};
use tracing::debug;
use tracing_subscriber::EnvFilter;

#[derive(Debug, clap::Parser)]
enum Args {
    CompressEachFrame {
        source: PathBuf,
        destination: PathBuf,
        #[arg(short, long)]
        metrics: Option<PathBuf>,
        #[arg(short, long, default_value_t = 0)]
        compression_level: u16,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();
    _main(clap::Parser::parse()).await
}

async fn _main(args: Args) -> anyhow::Result<()> {
    debug!(?args);
    match args {
        Args::CompressEachFrame {
            source,
            destination,
            metrics,
            compression_level,
        } => {
            let progress = MultiProgress::new();
            let metrics = OptionFuture::from(metrics.map(|path| async {
                let mut file = File::create(path).await?;
                file.write_all("uncompressed_len,compressed_len\n".as_bytes())
                    .await?;
                std::io::Result::Ok(file)
            }))
            .await
            .transpose()?
            .map(RefCell::new);

            let shrunk = progress.add(
                ProgressBar::new_spinner()
                    .with_style(count())
                    .with_message("blocks shrunk"),
            );
            let grown = progress.add(
                ProgressBar::new_spinner()
                    .with_style(count())
                    .with_message("blocks grown"),
            );
            let shrunk = &shrunk;
            let grown = &grown;
            let metrics = metrics.as_ref();

            let source = File::open(source).await?;
            let source = progress
                .add(
                    ProgressBar::new(source.metadata().await?.len())
                        .with_style(read())
                        .with_message("reading"),
                )
                .wrap_async_read(source);

            let source = FramedRead::new(
                source,
                unsigned_varint::codec::UviBytes::<bytes::BytesMut>::default(),
            );
            let destination = FramedWrite::new(
                progress
                    .add(
                        ProgressBar::new_spinner()
                            .with_style(write())
                            .with_message("writing"),
                    )
                    .wrap_async_write(File::create(destination).await?),
                unsigned_varint::codec::UviBytes::<std::io::Cursor<Vec<u8>>>::default(),
            );
            source
                .and_then(|uncompressed| async move {
                    let uncompressed_len = uncompressed.len();
                    let compressed =
                        zstd::encode_all(uncompressed.reader(), i32::from(compression_level))
                            .expect("BytesMut cannot emit io errors");
                    let compressed_len = compressed.len();
                    if let Some(metrics) = metrics {
                        metrics
                            .borrow_mut()
                            .write_all(
                                format!("{},{}\n", compressed_len, uncompressed_len).as_bytes(),
                            )
                            .await?;
                    }
                    match compressed_len > uncompressed_len {
                        true => grown.inc(1),
                        false => shrunk.inc(1),
                    }
                    Ok(std::io::Cursor::new(compressed))
                })
                .forward(destination)
                .await?;
        }
    }
    Ok(())
}

fn read() -> ProgressStyle {
    ProgressStyle::with_template(
        "{msg:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {bytes}/{total_bytes}",
    )
    .expect("invalid progress template")
    .progress_chars("=>-")
}

const TICK_STRINGS: &[&str] = &[
    "▹▹▹▹▹",
    "▸▹▹▹▹",
    "▹▸▹▹▹",
    "▹▹▸▹▹",
    "▹▹▹▸▹",
    "▹▹▹▹▸",
    "▪▪▪▪▪",
];

fn write() -> ProgressStyle {
    ProgressStyle::with_template("{spinner:.blue} {bytes} {msg}")
        .unwrap()
        .tick_strings(TICK_STRINGS)
}

fn count() -> ProgressStyle {
    ProgressStyle::with_template("{spinner:.blue} {pos} {msg}")
        .unwrap()
        .tick_strings(TICK_STRINGS)
}
