// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::chain::{
    index::{ChainIndex, ResolveNullTipset},
    ChainEpochDelta, ChainStore,
};
use crate::db::car::forest::DEFAULT_FOREST_CAR_FRAME_SIZE;
use crate::db::car::ManyCar;
use crate::db::db_utils::parity::TempParityDB;
use crate::db::{GarbageCollectable, MarkAndSweep};
use crate::genesis::read_genesis_header;
use crate::ipld::{stream_chain, stream_graph, unordered_stream_graph};
use crate::networks::ChainConfig;
use crate::shim::clock::ChainEpoch;
use crate::utils::db::car_stream::{CarBlock, CarStream};
use crate::utils::encoding::extract_cids;
use crate::utils::stream::par_buffer;
use ahash::{HashSet, HashSetExt};
use anyhow::Context as _;
use cid::Cid;
use clap::Subcommand;
use futures::{StreamExt, TryStreamExt};
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::DAG_CBOR;
use indicatif::{ProgressBar, ProgressStyle};
use itertools::Itertools;
use std::ops::Deref;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::{
    fs::File,
    io::{AsyncWrite, AsyncWriteExt, BufReader},
};
use tracing::info;

#[derive(Debug, Subcommand)]
pub enum BenchmarkCommands {
    /// Benchmark streaming data from a CAR archive
    CarStreaming {
        /// Snapshot input files (`.car.`, `.car.zst`, `.forest.car.zst`)
        #[arg(required = true)]
        snapshot_files: Vec<PathBuf>,
        /// Whether or not we want to expect [`libipld_core::ipld::Ipld`] data for each block.
        #[arg(long)]
        inspect: bool,
    },
    /// Benchmark GC
    GC {
        /// Snapshot input files (`.car.`, `.car.zst`, `.forest.car.zst`)
        #[arg(required = true)]
        snapshot_files: Vec<PathBuf>,
        /// Run a simulated GC sweep step, directly calling `db.remove_keys()`.\
        simulated_sweep: bool,
    },
    /// Depth-first traversal of the Filecoin graph
    GraphTraversal {
        /// Snapshot input files (`.car.`, `.car.zst`, `.forest.car.zst`)
        #[arg(required = true)]
        snapshot_files: Vec<PathBuf>,
    },
    // Unordered traversal of the Filecoin graph, yields blocks in an undefined order.
    UnorderedGraphTraversal {
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
        #[arg(long, default_value_t = DEFAULT_FOREST_CAR_FRAME_SIZE)]
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
        #[arg(long, default_value_t = DEFAULT_FOREST_CAR_FRAME_SIZE)]
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
    pub async fn run(self) -> anyhow::Result<()> {
        match self {
            Self::CarStreaming {
                snapshot_files,
                inspect,
            } => match inspect {
                true => benchmark_car_streaming_inspect(snapshot_files).await,
                false => benchmark_car_streaming(snapshot_files).await,
            },
            Self::GraphTraversal { snapshot_files } => {
                benchmark_graph_traversal(snapshot_files).await
            }
            Self::UnorderedGraphTraversal { snapshot_files } => {
                benchmark_unordered_graph_traversal(snapshot_files).await
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
            Self::GC {
                snapshot_files,
                simulated_sweep,
            } => match simulated_sweep {
                true => benchmark_sim_gc_sweep(snapshot_files).await,
                false => benchmark_gc(snapshot_files).await,
            },
        }
    }
}

// Load a set of CAR files into ParityDB and run GC.
async fn benchmark_gc(input: Vec<PathBuf>) -> anyhow::Result<()> {
    let mut sink = indicatif_sink("populated");

    let temp_db = Arc::new(TempParityDB::new());
    let db = temp_db.arc();

    let mut s = Box::pin(
        futures::stream::iter(input)
            .then(File::open)
            .map_ok(BufReader::new)
            .and_then(CarStream::new)
            .try_flatten(),
    );
    info!("populating temp db");
    while let Some(block) = s.try_next().await? {
        db.put_keyed(&block.cid, &block.data)?;
        sink.write_all(&block.data).await?;
    }
    info!("finished populating temp db");

    let config = Arc::new(ChainConfig::mainnet());
    let genesis_header =
        read_genesis_header(None, config.genesis_bytes(&db).await?.as_deref(), &db).await?;

    let cs = ChainStore::new(
        db.clone(),
        db.clone(),
        db.clone(),
        config.clone(),
        genesis_header,
    )?;

    let mut chain_arc = cs.heaviest_tipset().chain_arc(db.clone());
    // Make sure we have enough garbage to collect.
    let get_heaviest_tipset = Box::new(move || chain_arc.next().unwrap());
    let depth = 0;
    let interval = std::time::Duration::from_secs(0);

    let mut gc = MarkAndSweep::new(db.clone(), get_heaviest_tipset, depth, interval);

    info!("marking keys for deletion");
    gc.gc_workflow(interval).await?;
    info!("marked keys for deletion");

    info!("filter and sweep");
    gc.gc_workflow(interval).await?;
    info!("finished filter and sweep");

    Ok(())
}

// Load a set of CAR files into ParityDB and see how long it takes to run the simulated GC sweep.
async fn benchmark_sim_gc_sweep(input: Vec<PathBuf>) -> anyhow::Result<()> {
    let mut sink = indicatif_sink("populated");
    use crate::db::db_utils::parity::TempParityDB;

    let temp_db = Arc::new(TempParityDB::new());
    let mut s = Box::pin(
        futures::stream::iter(input)
            .then(File::open)
            .map_ok(BufReader::new)
            .and_then(CarStream::new)
            .try_flatten(),
    );

    info!("populating temp db");
    while let Some(block) = s.try_next().await? {
        temp_db.put_keyed(&block.cid, &block.data)?;
        sink.write_all(&block.data).await?;
    }
    info!("finished populating temp db");

    info!("removing keys");
    let _ = temp_db.remove_keys(HashSet::new())?;
    info!("finished removing keys");

    Ok(())
}

// Concatenate a set of CAR files and measure how quickly we can stream the
// blocks.
async fn benchmark_car_streaming(input: Vec<PathBuf>) -> anyhow::Result<()> {
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

// Concatenate a set of CAR files and measure how quickly we can stream the
// blocks, while inspecting them. This a benchmark we could use for setting
// realistic expectations in terms of DFS graph travels, for example.
async fn benchmark_car_streaming_inspect(input: Vec<PathBuf>) -> anyhow::Result<()> {
    let mut sink = indicatif_sink("traversed");
    let mut s = Box::pin(
        futures::stream::iter(input)
            .then(File::open)
            .map_ok(BufReader::new)
            .and_then(CarStream::new)
            .try_flatten(),
    );
    while let Some(block) = s.try_next().await? {
        let block: CarBlock = block;
        if block.cid.codec() == DAG_CBOR {
            let cid_vec: Vec<Cid> = extract_cids(&block.data)?;
            let _ = cid_vec.iter().unique().count();
        }
        sink.write_all(&block.data).await?
    }
    Ok(())
}

// Open a set of CAR files as a block store and do a DFS traversal of all
// reachable nodes.
async fn benchmark_graph_traversal(input: Vec<PathBuf>) -> anyhow::Result<()> {
    let store = open_store(input)?;
    let heaviest = store.heaviest_tipset()?;

    let mut sink = indicatif_sink("traversed");

    let mut s = stream_graph(&store, heaviest.chain(&store), 0);
    while let Some(block) = s.try_next().await? {
        sink.write_all(&block.data).await?
    }

    Ok(())
}

// Open a set of CAR files as a block store and do an unordered traversal of all
// reachable nodes.
async fn benchmark_unordered_graph_traversal(input: Vec<PathBuf>) -> anyhow::Result<()> {
    let store = Arc::new(open_store(input)?);
    let heaviest = store.heaviest_tipset()?;

    let mut sink = indicatif_sink("traversed");

    let mut s = unordered_stream_graph(store.clone(), heaviest.chain(store), 0);
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
) -> anyhow::Result<()> {
    let file = tokio::io::BufReader::new(File::open(&input).await?);

    let mut block_stream = CarStream::new(file).await?;
    let roots = std::mem::replace(
        &mut block_stream.header.roots,
        nunny::vec![Default::default()],
    );

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
) -> anyhow::Result<()> {
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
    crate::db::car::forest::Encoder::write(&mut dest, ts.key().to_cids(), frames).await?;
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
fn open_store(input: Vec<PathBuf>) -> anyhow::Result<ManyCar> {
    let pb = indicatif::ProgressBar::new_spinner().with_style(
        indicatif::ProgressStyle::with_template("{spinner} opening block store")
            .expect("indicatif template must be valid"),
    );
    pb.enable_steady_tick(std::time::Duration::from_secs_f32(0.1));

    let store = ManyCar::try_from(input).context("couldn't read input CAR file")?;

    pb.finish_and_clear();

    Ok(store)
}
