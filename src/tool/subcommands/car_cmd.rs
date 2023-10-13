// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::path::PathBuf;

use clap::Subcommand;
use futures::{StreamExt, TryStreamExt};
use fvm_ipld_blockstore::Blockstore;
use indicatif::{ProgressBar, ProgressStyle};
use itertools::Itertools;
use tokio::{
    fs::File,
    io::{AsyncWriteExt, BufReader},
};

use crate::db::car::ForestCar;
use crate::utils::db::{
    car_stream::CarStream,
    car_util::{dedup_block_stream, merge_car_streams},
};

#[derive(Debug, Subcommand)]
pub enum CarCommands {
    /// Concatenate two or more CAR files into a single archive
    Concat {
        /// A list of CAR file paths. A CAR file can be a plain CAR, a zstd compressed CAR
        /// or a `.forest.car.zst` file
        car_files: Vec<PathBuf>,
        /// The output `.forest.car.zst` file path
        #[arg(short, long)]
        output: PathBuf,
    },
    /// Check the validity of a CAR archive. For Filecoin-specific checks, see
    /// `forest-tool snapshot validate`.
    Validate {
        /// CAR archive. Supported extensions: `.car`, `.car.zst`, `.forest.car.zst`
        car_file: PathBuf,
        /// Skip verifying that blocks are hashed correctly
        #[arg(long)]
        ignore_block_validity: bool,
        /// Skip verifying the integrity of the on-disk index
        #[arg(long)]
        ignore_forest_index: bool,
    },
}

impl CarCommands {
    pub async fn run(self) -> anyhow::Result<()> {
        match self {
            Self::Concat { car_files, output } => {
                let car_streams: Vec<_> = futures::stream::iter(car_files)
                    .then(tokio::fs::File::open)
                    .map_ok(tokio::io::BufReader::new)
                    .and_then(CarStream::new)
                    .try_collect()
                    .await?;

                let all_roots = car_streams
                    .iter()
                    .flat_map(|it| it.header.roots.iter())
                    .unique()
                    .cloned()
                    .collect::<Vec<_>>();

                let frames = crate::db::car::forest::Encoder::compress_stream_default(
                    dedup_block_stream(merge_car_streams(car_streams)).map_err(anyhow::Error::from),
                );
                let mut writer = tokio::io::BufWriter::new(tokio::fs::File::create(&output).await?);
                crate::db::car::forest::Encoder::write(&mut writer, all_roots, frames).await?;
                writer.flush().await?;
            }
            Self::Validate {
                car_file,
                ignore_block_validity: check_block_validity,
                ignore_forest_index: check_forest_index,
            } => validate(car_file, check_block_validity, check_forest_index).await?,
        }
        Ok(())
    }
}

// At present, three invariants are checked:
// - The CAR file is syntactically valid and all blocks can be streamed.
// - Each block CID is checked against the hash of the block.
// - Each block CID is looked-up in the on-disk index.
//
// We do not check for duplicate blocks. Whether duplicate blocks are allowed or
// not is vague in the specification.
async fn validate(
    car_file: PathBuf,
    ignore_block_validity: bool,
    ignore_forest_index: bool,
) -> anyhow::Result<()> {
    let optional_db = if !ignore_forest_index {
        Some(ForestCar::try_from(car_file.as_path())?)
    } else {
        None
    };

    let file = File::open(car_file).await?;
    let pb = ProgressBar::new(file.metadata().await?.len()).with_style(
        ProgressStyle::with_template("{bar} {percent}%, eta: {eta}").expect("infallible"),
    );
    let file = BufReader::new(pb.wrap_async_read(file));

    let mut stream = CarStream::new(file).await?;
    while let Some(block) = stream.try_next().await? {
        if !ignore_block_validity && !block.valid() {
            anyhow::ensure!(block.valid(), "CID/Block mismatch for block: {}", block.cid);
        }
        if let Some(ref db) = optional_db {
            anyhow::ensure!(db.get(&block.cid).ok().flatten() == Some(block.data));
        }
    }
    Ok(())
}
