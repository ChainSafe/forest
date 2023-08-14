// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::chain::{
    index::{ChainIndex, ResolveNullTipset},
    ChainEpochDelta,
};
use crate::db::car::ManyCar;
use crate::ipld::{stream_chain, stream_graph};
use crate::shim::clock::ChainEpoch;
use crate::utils::db::car_stream::CarStream;
use crate::utils::stream::par_buffer;
use anyhow::{Context as _, Result};
use cid::Cid;
use clap::Subcommand;
use futures::{StreamExt, TryStreamExt};
use indicatif::{ProgressBar, ProgressStyle};
use std::ops::Deref;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::{
    fs::File,
    io::{AsyncWrite, AsyncWriteExt, BufReader},
};

#[derive(Debug, Subcommand)]
pub enum BenchmarkCommands {
    /// Benchmark streaming data from a CAR archive
    CarStreaming {
        /// Snapshot input files (`.car.`, `.car.zst`, `.forest.car.zst`)
        #[arg(required = true)]
        snapshot_files: Vec<PathBuf>,
    },
    /// Depth-first traversal of the Filecoin graph
    GraphTraversal {
        /// Snapshot input files (`.car.`, `.car.zst`, `.forest.car.zst`)
        #[arg(required = true)]
        snapshot_files: Vec<PathBuf>,
    },
    /// Encoding of a `.forest.car.zst` file
    ForestEncoding {
        /// Snapshot input file (`.car.`, `.car.zst`, `.forest.car.zst`)
        snapshot_file: PathBuf,
        #[arg(long, default_value_t = 3)]
        compression_level: u16,
        /// End zstd frames after they exceed this length
        #[arg(long, default_value_t = 8000usize.next_power_of_two())]
        frame_size: usize,
    },
    /// Exporting a `.forest.car.zst` file from HEAD
    Export {
        /// Snapshot input files (`.car.`, `.car.zst`, `.forest.car.zst`)
        #[arg(required = true)]
        snapshot_files: Vec<PathBuf>,
        #[arg(long, default_value_t = 3)]
        compression_level: u16,
        /// End zstd frames after they exceed this length
        #[arg(long, default_value_t = 8000usize.next_power_of_two())]
        frame_size: usize,
        /// Latest epoch that has to be exported for this snapshot, the upper bound. This value
        /// cannot be greater than the latest epoch available in the input snapshot.
        #[arg(short, long)]
        epoch: Option<ChainEpoch>,
        /// How many state-roots to include. Lower limit is 900 for `calibnet` and `mainnet`.
        #[arg(short, long, default_value_t = 2000)]
        depth: ChainEpochDelta,
    },
}

impl BenchmarkCommands {
    pub async fn run(self) -> Result<()> {
        match self {
            Self::CarStreaming { snapshot_files } => benchmark_car_streaming(snapshot_files).await,
            Self::GraphTraversal { snapshot_files } => {
                benchmark_graph_traversal(snapshot_files).await
            }
            Self::ForestEncoding {
                snapshot_file,
                compression_level,
                frame_size,
            } => benchmark_forest_encoding(snapshot_file, compression_level, frame_size).await,
            Self::Export {
                snapshot_files,
                compression_level,
                frame_size,
                epoch,
                depth,
            } => {
                benchmark_exporting(snapshot_files, compression_level, frame_size, epoch, depth)
                    .await
            }
        }
    }
}

// Concatenate a set of CAR files and measure how quickly we can stream the
// blocks.
async fn benchmark_car_streaming(input: Vec<PathBuf>) -> Result<()> {
    let mut sink = indicatif_sink("traversed");

    let mut s = Box::pin(
        futures::stream::iter(input)
            .then(File::open)
            .map_ok(BufReader::new)
            .and_then(CarStream::new)
            .try_flatten(),
    );
    while let Some(block) = s.try_next().await? {
        sink.write_all(&block.data).await?
    }
    Ok(())
}

// Open a set of CAR files as a block store and do a DFS traversal of all
// reachable nodes.
async fn benchmark_graph_traversal(input: Vec<PathBuf>) -> Result<()> {
    let store = open_store(input)?;
    let heaviest = store.heaviest_tipset()?;

    let mut sink = indicatif_sink("traversed");

    let mut s = stream_graph(&store, heaviest.chain(&store));
    while let Some(block) = s.try_next().await? {
        sink.write_all(&block.data).await?
    }
    Ok(())
}

// Encode a file to the ForestCAR.zst format and measure throughput.
async fn benchmark_forest_encoding(
    input: PathBuf,
    compression_level: u16,
    frame_size: usize,
) -> Result<()> {
    let file = tokio::io::BufReader::new(File::open(&input).await?);

    let mut block_stream = CarStream::new(file).await?;
    let roots = std::mem::take(&mut block_stream.header.roots);

    let mut dest = indicatif_sink("encoded");

    let frames = crate::db::car::forest::Encoder::compress_stream(
        frame_size,
        compression_level,
        par_buffer(1024, block_stream.map_err(anyhow::Error::from)),
    );
    crate::db::car::forest::Encoder::write(&mut dest, roots, frames).await?;
    dest.flush().await?;
    Ok(())
}

// Exporting combines a graph traversal with ForestCAR.zst encoding. Ideally, it
// should be no lower than `min(benchmark_graph_traversal,
// benchmark_forest_encoding)`.
async fn benchmark_exporting(
    input: Vec<PathBuf>,
    compression_level: u16,
    frame_size: usize,
    epoch: Option<ChainEpoch>,
    depth: ChainEpochDelta,
) -> Result<()> {
    let store = Arc::new(open_store(input)?);
    let heaviest = store.heaviest_tipset()?;
    let idx = ChainIndex::new(&store);
    let ts = idx.tipset_by_height(
        epoch.unwrap_or(heaviest.epoch()),
        Arc::new(heaviest),
        ResolveNullTipset::TakeOlder,
    )?;
    // We don't do any sanity checking for 'depth'. The output is discarded so
    // there's no need.
    let stateroot_lookup_limit = ts.epoch() - depth;

    let mut dest = indicatif_sink("exported");

    let blocks = stream_chain(
        Arc::clone(&store),
        ts.deref().clone().chain(Arc::clone(&store)),
        stateroot_lookup_limit,
    );

    let frames = crate::db::car::forest::Encoder::compress_stream(
        frame_size,
        compression_level,
        par_buffer(1024, blocks.map_err(anyhow::Error::from)),
    );
    crate::db::car::forest::Encoder::write(&mut dest, Vec::<Cid>::from(&ts.key().cids), frames)
        .await?;
    dest.flush().await?;
    Ok(())
}

// Sink with attached progress indicator
fn indicatif_sink(task: &'static str) -> impl AsyncWrite {
    let sink = tokio::io::sink();
    let pb = ProgressBar::new_spinner()
        .with_style(
            ProgressStyle::with_template(
                "{spinner} {prefix} {total_bytes} at {binary_bytes_per_sec} in {elapsed_precise}",
            )
            .expect("infallible"),
        )
        .with_prefix(task)
        .with_finish(indicatif::ProgressFinish::AndLeave);
    pb.enable_steady_tick(std::time::Duration::from_secs_f32(0.1));
    pb.wrap_async_write(sink)
}

// Opening a block store may take a long time (CAR files have to be indexed,
// CAR.zst files have to be decompressed). Show a progress indicator and clear
// it when done.
fn open_store(input: Vec<PathBuf>) -> Result<ManyCar> {
    let pb = indicatif::ProgressBar::new_spinner().with_style(
        indicatif::ProgressStyle::with_template("{spinner} opening block store")
            .expect("indicatif template must be valid"),
    );
    pb.enable_steady_tick(std::time::Duration::from_secs_f32(0.1));

    let store = ManyCar::try_from(input).context("couldn't read input CAR file")?;

    pb.finish_and_clear();

    Ok(store)
}
