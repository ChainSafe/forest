use indicatif::{MultiProgress, ProgressBar, ProgressFinish, ProgressStyle};
use std::{fmt::Display, path::PathBuf};
use tokio::fs::File;
use tracing::debug;
use tracing_subscriber::EnvFilter;

#[derive(Debug, clap::Parser)]
enum Args {
    /// Each zstd frame will contain a whole number of varint frames
    CompressManyframe {
        source: PathBuf,
        destination: PathBuf,
        #[arg(short, long, default_value_t = 3)]
        compression_level: u16,
        /// End zstd frames after they exceed this length
        #[arg(short, long, default_value_t = 8000usize.next_power_of_two())]
        zstd_frame_size_tripwire: usize,
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
        Args::CompressManyframe {
            source,
            destination,
            compression_level,
            zstd_frame_size_tripwire,
        } => {
            let progress = MultiProgress::new();

            let source = File::open(source).await?;
            let source = progress
                .add(reader_of("reading CAR", source.metadata().await?.len()))
                .wrap_async_read(source);
            let destination = progress
                .add(writer_of("writing reframed CAR"))
                .wrap_async_write(File::create(destination).await?);

            forest_filecoin::zstd_compress_varint_manyframe(
                source,
                destination,
                zstd_frame_size_tripwire,
                compression_level,
            )
            .await?;
        }
    }
    Ok(())
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

fn writer_of(message: impl Display) -> ProgressBar {
    ProgressBar::new_spinner()
        .with_style(write_style())
        .with_message(message.to_string())
        .with_finish(ProgressFinish::AndLeave)
}

fn reader_of(message: impl Display, length: impl Into<Option<u64>>) -> ProgressBar {
    let pb = match length.into() {
        Some(len) => ProgressBar::new(len),
        None => ProgressBar::new_spinner(),
    };
    pb.with_message(message.to_string())
        .with_style(read_style())
}

fn read_style() -> ProgressStyle {
    ProgressStyle::with_template(
        "{msg:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {bytes}/{total_bytes}",
    )
    .expect("invalid progress template")
    .progress_chars("=>-")
}

fn write_style() -> ProgressStyle {
    ProgressStyle::with_template("{spinner:.blue} {bytes} {msg}")
        .unwrap()
        .tick_strings(TICK_STRINGS)
}
