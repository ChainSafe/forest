// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::path::{Path, PathBuf};

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
                ignore_block_validity,
                ignore_forest_index,
            } => validate(&car_file, ignore_block_validity, ignore_forest_index).await?,
        }
        Ok(())
    }
}

/// At present, three properties are checked:
/// - The CAR file is syntactically valid and all blocks can be streamed.
/// - Each block CID is checked against the hash of the block.
/// - Each block CID is looked-up in the on-disk index.
///
/// Properties related to Filecoin are not checked. For those, see `forest-tool
/// snapshot validate`.
///
/// We do not check for duplicate blocks. Whether duplicate blocks are allowed or
/// not is vague in the specification.
async fn validate(
    car_file: &Path,
    ignore_block_validity: bool,
    ignore_forest_index: bool,
) -> anyhow::Result<()> {
    let optional_db = if !ignore_forest_index {
        Some(ForestCar::try_from(car_file)?)
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

#[cfg(test)]
mod tests {
    use super::validate;
    use crate::db::car::forest;
    use crate::networks::{calibnet, mainnet};
    use crate::utils::db::car_stream::CarBlock;
    use cid::multihash::{Code, MultihashDigest};
    use cid::Cid;
    use futures::{stream::iter, StreamExt, TryStreamExt};
    use std::io::Write;
    use tempfile::{Builder, TempPath};
    use tokio::io::AsyncWriteExt;

    #[tokio::test(flavor = "multi_thread")]
    async fn validate_junk_car() {
        let mut temp_path = Builder::new().tempfile().unwrap();
        temp_path.write_all(&[0xde, 0xad, 0xbe, 0xef]).unwrap();
        assert!(validate(&temp_path.into_temp_path(), false, false)
            .await
            .is_err());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn validate_empty_car() {
        let temp_path = Builder::new().tempfile().unwrap();
        assert!(validate(&temp_path.into_temp_path(), false, false)
            .await
            .is_err());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn validate_mainnet_genesis() {
        let mut temp_path = Builder::new().tempfile().unwrap();
        temp_path.write_all(mainnet::DEFAULT_GENESIS).unwrap();
        assert!(validate(&temp_path.into_temp_path(), false, true)
            .await
            .is_ok());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn validate_calibnet_genesis() {
        let mut temp_path = tempfile::Builder::new().tempfile().unwrap();
        temp_path.write_all(calibnet::DEFAULT_GENESIS).unwrap();
        assert!(validate(&temp_path.into_temp_path(), false, true)
            .await
            .is_ok());
    }

    fn valid_block(msg: &str) -> CarBlock {
        let data = msg.as_bytes().to_vec();
        CarBlock {
            cid: Cid::new_v1(0, Code::Blake2b256.digest(&data)),
            data,
        }
    }

    fn invalid_block(msg: &str) -> CarBlock {
        let cid = Cid::new_v1(0, Code::Identity.digest(&[]));
        let data = msg.as_bytes().to_vec();
        CarBlock { cid, data }
    }

    async fn create_raw_car_file(car_blocks: Vec<CarBlock>, ignored_cids: Vec<Cid>) -> TempPath {
        let temp_path = Builder::new().tempfile().unwrap().into_temp_path();
        let mut writer = tokio::fs::File::create(&temp_path).await.unwrap();

        let frames = forest::Encoder::compress_stream_default(iter(car_blocks).map(Ok)).map_ok(
            |(cids, bytes)| {
                (
                    cids.into_iter()
                        .filter(|cid| !ignored_cids.contains(cid))
                        .collect(),
                    bytes,
                )
            },
        );

        // Write zstd frames and include a skippable index
        forest::Encoder::write(&mut writer, vec![], frames)
            .await
            .unwrap();

        // Flush to ensure everything has been successfully written
        writer.flush().await.unwrap();
        writer.shutdown().await.unwrap();
        temp_path
    }

    // Sanity check to verify that we can create valid forest.car.zst files
    #[tokio::test(flavor = "multi_thread")]
    async fn validate_valid_file() {
        let temp_path =
            create_raw_car_file(vec![valid_block("this data _does_ match the CID")], vec![]).await;

        assert!(validate(&temp_path, false, false).await.is_ok());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn validate_invalid_blocks() {
        let temp_path = create_raw_car_file(
            vec![
                valid_block("car_stream checks the first block"),
                invalid_block("this data doesn't match the CID"),
            ],
            vec![],
        )
        .await;

        assert!(validate(&temp_path, false, false).await.is_err());
        // Ignoring block validity and index validity should make the test pass.
        assert!(validate(&temp_path, true, false).await.is_ok());
    }

    // If a CarBlock exist that isn't referenced in the index, this is an error.
    #[tokio::test(flavor = "multi_thread")]
    async fn validate_invalid_index() {
        let block = valid_block("this data _does_ match the CID");
        let temp_path = create_raw_car_file(vec![block.clone()], vec![block.cid]).await;

        assert!(validate(&temp_path, false, false).await.is_err());
        // Ignoring index validity should make the test pass.
        assert!(validate(&temp_path, false, true).await.is_ok());
    }
}
