// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod common;

use std::path::Path;

use anyhow::*;
use cid::{
    multihash::{self, MultihashDigest},
    Cid,
};
use forest_filecoin::utils::db::car_stream::{Block, CarStream};
use futures::{StreamExt, TryStreamExt};
use fvm_ipld_encoding::DAG_CBOR;
use itertools::Itertools;
use rand::{rngs::SmallRng, Rng, SeedableRng};
use tokio::io::AsyncWriteExt;

use crate::common::cli;

const FOREST_CAR_ZST_SUFFIX: &str = ".forest.car.zst";

#[tokio::test]
async fn forest_cli_car_concat() -> Result<()> {
    let a = tempfile::Builder::new()
        .prefix("forest-cli-car-concat-a")
        .suffix(FOREST_CAR_ZST_SUFFIX)
        .tempfile()?;
    new_car(1024, a.path()).await?;

    let b = tempfile::Builder::new()
        .prefix("forest-cli-car-concat-b")
        .suffix(FOREST_CAR_ZST_SUFFIX)
        .tempfile()?;
    new_car(2048, b.path()).await?;

    let output = tempfile::Builder::new()
        .prefix("forest-cli-car-concat-output")
        .suffix(FOREST_CAR_ZST_SUFFIX)
        .tempfile()?;

    cli()?
        .arg("car")
        .arg("concat")
        .arg(a.path().as_os_str().to_str().unwrap())
        .arg(b.path().as_os_str().to_str().unwrap())
        .arg("-o")
        .arg(output.path().as_os_str().to_str().unwrap())
        .assert()
        .success();

    validate_car(output.path()).await?;

    Ok(())
}

#[tokio::test]
async fn forest_cli_car_concat_same_file() -> Result<()> {
    let output = tempfile::Builder::new()
        .prefix("forest-cli-car-concat-same-file")
        .suffix(FOREST_CAR_ZST_SUFFIX)
        .tempfile()?;

    cli()?
        .arg("car")
        .arg("concat")
        .arg("./test-snapshots/chain4.car")
        .arg("./test-snapshots/chain4.car")
        .arg("-o")
        .arg(output.path().as_os_str().to_str().unwrap())
        .assert()
        .success();

    validate_car(output.path()).await?;

    Ok(())
}

#[tokio::test]
async fn forest_cli_car_concat_same_file_3_times() -> Result<()> {
    let output = tempfile::Builder::new()
        .prefix("forest-cli-car-concat-same-file-3-times")
        .suffix(FOREST_CAR_ZST_SUFFIX)
        .tempfile()?;

    cli()?
        .arg("car")
        .arg("concat")
        .arg("./test-snapshots/chain4.car")
        .arg("./test-snapshots/chain4.car")
        .arg("./test-snapshots/chain4.car")
        .arg("-o")
        .arg(output.path().as_os_str().to_str().unwrap())
        .assert()
        .success();

    validate_car(output.path()).await?;

    Ok(())
}

async fn new_car(size: usize, path: impl AsRef<Path>) -> Result<()> {
    let rng = SmallRng::seed_from_u64(0xdeadbeef);
    let root_block = new_block(&mut rng.clone());
    let roots = vec![root_block.cid];
    let block_stream = futures::stream::iter(vec![Ok(root_block)]).chain(
        futures::stream::unfold(rng, |mut rng| async {
            Some((Ok(new_block(&mut rng)), rng))
        })
        .take(size.saturating_sub(1)),
    );
    let frames = forest_filecoin::db::car::forest::Encoder::compress_stream(
        8000_usize.next_power_of_two(),
        zstd::DEFAULT_COMPRESSION_LEVEL as _,
        block_stream,
    );
    let mut writer = tokio::io::BufWriter::new(tokio::fs::File::create(path).await?);
    forest_filecoin::db::car::forest::Encoder::write(&mut writer, roots, frames).await?;
    writer.flush().await?;

    Ok(())
}

fn new_block(rng: &mut SmallRng) -> Block {
    let mut data = [0; 64];
    rng.fill(&mut data);
    let cid = Cid::new_v1(DAG_CBOR, multihash::Code::Blake2b256.digest(&data));
    Block {
        cid,
        data: data.to_vec(),
    }
}

async fn validate_car(path: impl AsRef<Path>) -> Result<()> {
    let reader = CarStream::new(tokio::io::BufReader::new(
        tokio::fs::File::open(path).await?,
    ))
    .await?;
    assert!(!reader.header.roots.is_empty());
    let mut count = 0;
    let blocks: Vec<_> = reader.inspect_ok(|_| count += 1).try_collect().await?;
    assert!(blocks.iter().map(|b| b.cid).all_unique());
    println!("Result car block count: {count}");
    Ok(())
}
