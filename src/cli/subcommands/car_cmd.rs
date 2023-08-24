// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::path::PathBuf;

use clap::Subcommand;
use futures::{StreamExt, TryStreamExt};
use itertools::Itertools;
use tokio::io::AsyncWriteExt;

use crate::utils::db::{
    car_stream::CarStream,
    car_util::{dedup_block_stream, merge_car_streams},
};

#[derive(Debug, Subcommand)]
pub enum CarCommands {
    Concat {
        /// A list of CAR file paths. A CAR file can be a plain CAR, a zstd compressed CAR
        /// or a `.forest.car.zst` file
        car_files: Vec<PathBuf>,
        /// The output `.forest.car.zst` file path
        #[arg(short, long)]
        output: PathBuf,
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

                let frames = crate::db::car::forest::Encoder::compress_stream(
                    8000_usize.next_power_of_two(),
                    zstd::DEFAULT_COMPRESSION_LEVEL as _,
                    dedup_block_stream(merge_car_streams(car_streams)).map_err(anyhow::Error::from),
                );
                let mut writer = tokio::io::BufWriter::new(tokio::fs::File::create(&output).await?);
                crate::db::car::forest::Encoder::write(&mut writer, all_roots, frames).await?;
                writer.flush().await?;
            }
        }
        Ok(())
    }
}
