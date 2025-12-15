// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::blocks::{Tipset, TipsetKey};
use crate::db::car::ManyCar;
use crate::db::car::forest::DEFAULT_FOREST_CAR_FRAME_SIZE;
use crate::ipld::{stream_chain, stream_graph};
use crate::shim::clock::ChainEpoch;
use crate::utils::db::car_stream::{CarBlock, CarStream};
use crate::utils::encoding::extract_cids;
use crate::utils::multihash::MultihashCode;
use crate::utils::stream::par_buffer;
use crate::{
    chain::{
        ChainEpochDelta,
        index::{ChainIndex, ResolveNullTipset},
    },
    db::{Blockstore, Either, parity_db::ParityDb, parity_db_config::ParityDbConfig},
};
use anyhow::Context as _;
use cid::Cid;
use clap::Subcommand;
use futures::{StreamExt, TryStreamExt};
use fvm_ipld_encoding::DAG_CBOR;
use human_repr::HumanCount as _;
use indicatif::{ProgressBar, ProgressStyle};
use itertools::Itertools;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;
use tokio::{
    fs::File,
    io::{AsyncWrite, AsyncWriteExt, BufReader},
};

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum DbType {
    Parity,
    ParityOpt,
}

#[derive(Debug, Subcommand)]
pub enum BenchmarkCommands {
    /// Benchmark streaming data from a CAR archive
    CarStreaming {
        /// Snapshot input files (`.car.`, `.car.zst`, `.forest.car.zst`)
        #[arg(required = true)]
        snapshot_files: Vec<PathBuf>,
        /// Whether or not we want to expect [`ipld_core::ipld::Ipld`] data for each block.
        #[arg(long)]
        inspect: bool,
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
    /// Benchmark key-value blockstore
    Blockstore {
        /// Snapshot input file (`.car.`, `.car.zst`, `.forest.car.zst`)
        #[arg(required = true)]
        snapshot_file: PathBuf,
        #[arg(long, default_value = "parity")]
        db: DbType,
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
            Self::Blockstore { snapshot_file, db } => benchmark_blockstore(snapshot_file, db).await,
        }
    }
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
            let cid_vec = extract_cids(&block.data)?;
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

// Encode a file to the ForestCAR.zst format and measure throughput.
async fn benchmark_forest_encoding(
    input: PathBuf,
    compression_level: u16,
    frame_size: usize,
) -> anyhow::Result<()> {
    let file = tokio::io::BufReader::new(File::open(&input).await?);

    let mut block_stream = CarStream::new(file).await?;
    let roots = std::mem::replace(
        &mut block_stream.header_v1.roots,
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
        heaviest,
        ResolveNullTipset::TakeOlder,
    )?;
    // We don't do any sanity checking for 'depth'. The output is discarded so
    // there's no need.
    let stateroot_lookup_limit = ts.epoch() - depth;

    let mut dest = indicatif_sink("exported");

    let blocks = stream_chain(
        Arc::clone(&store),
        ts.clone().chain_owned(Arc::clone(&store)),
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

async fn benchmark_blockstore(snapshot: PathBuf, db: DbType) -> anyhow::Result<()> {
    let tmp_db_path = tempfile::tempdir()?;
    let bs = open_blockstore(&db, tmp_db_path.path())?;
    let head_tsk = benchmark_blockstore_import(&snapshot, &db, &bs, tmp_db_path.path()).await?;
    benchmark_blockstore_traversal(&bs, &head_tsk).await?;
    Ok(())
}

fn open_blockstore(db: &DbType, db_path: &Path) -> anyhow::Result<impl Blockstore> {
    println!("temp db path: {}", db_path.display());
    Ok(match db {
        DbType::Parity => Either::Left(ParityDb::open(db_path, &ParityDbConfig::default())?),
        DbType::ParityOpt => Either::Right(ParityDbOpt::open(db_path)?),
    })
}

async fn benchmark_blockstore_import(
    snapshot: &Path,
    db: &DbType,
    bs: &impl Blockstore,
    bs_path: &Path,
) -> anyhow::Result<TipsetKey> {
    let mut car_stream = CarStream::new_from_path(snapshot).await?;
    let head_tsk = car_stream.head_tipset_key();
    println!("head tipset key: {head_tsk}");
    println!("importing CAR into {db:?} blockstore...");
    let start = Instant::now();
    let mut n = 0;
    while let Some(CarBlock { cid, data }) = car_stream.try_next().await? {
        bs.put_keyed(&cid, &data)?;
        n += 1;
    }
    let db_size = fs_extra::dir::get_size(bs_path).unwrap_or_default();
    println!(
        "imported {n} records into {db:?} blockstore(size={}), took {}",
        db_size.human_count_bytes(),
        humantime::format_duration(start.elapsed())
    );
    Ok(head_tsk)
}

async fn benchmark_blockstore_traversal(
    bs: &impl Blockstore,
    head_tsk: &TipsetKey,
) -> anyhow::Result<()> {
    println!("Traversing the chain...");
    let head = Tipset::load_required(bs, head_tsk)?;
    let mut sink = indicatif_sink("traversed");
    let start = Instant::now();
    let mut s = stream_graph(bs, head.chain(bs), 0);
    let mut n = 0;
    while let Some(block) = s.try_next().await? {
        sink.write_all(&block.data).await?;
        n += 1;
    }
    println!(
        "Traversed {n} records, took {}",
        humantime::format_duration(start.elapsed())
    );
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

struct ParityDbOpt {
    db: parity_db::Db,
}

impl ParityDbOpt {
    fn open(path: impl Into<PathBuf>) -> anyhow::Result<Self> {
        let opts = parity_db::Options {
            path: path.into(),
            sync_wal: true,
            sync_data: true,
            stats: false,
            salt: None,
            columns: vec![
                parity_db::ColumnOptions {
                    preimage: true,
                    uniform: true,
                    compression: parity_db::CompressionType::Lz4,
                    ..Default::default()
                },
                parity_db::ColumnOptions {
                    preimage: true,
                    compression: parity_db::CompressionType::Lz4,
                    ..Default::default()
                },
            ],
            compression_threshold: [(0, 128)].into_iter().collect(),
        };
        let db = parity_db::Db::open_or_create(&opts)?;
        Ok(Self { db })
    }
}

impl Blockstore for ParityDbOpt {
    fn get(&self, k: &cid::Cid) -> anyhow::Result<Option<Vec<u8>>> {
        Ok(if is_dag_cbor_blake2b256(k) {
            self.db.get(0, k.hash().digest())?
        } else {
            self.db.get(1, &k.to_bytes())?
        })
    }

    fn put_keyed(&self, k: &cid::Cid, block: &[u8]) -> anyhow::Result<()> {
        if is_dag_cbor_blake2b256(k) {
            self.db
                .commit([(0, k.hash().digest(), Some(block.to_vec()))])?;
        } else {
            self.db.commit([(1, k.to_bytes(), Some(block.to_vec()))])?;
        }
        Ok(())
    }
}

fn is_dag_cbor_blake2b256(cid: &Cid) -> bool {
    cid.codec() == DAG_CBOR && cid.hash().code() == u64::from(MultihashCode::Blake2b256)
}
