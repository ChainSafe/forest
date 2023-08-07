// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod common;

use std::path::Path;

use anyhow::*;
use cid::{
    multihash::{self, MultihashDigest},
    Cid,
};
use futures::StreamExt;
use fvm_ipld_car::{CarHeader, CarReader};
use fvm_ipld_encoding::DAG_CBOR;
use rand::{rngs::SmallRng, Rng, SeedableRng};
use tokio_util::compat::TokioAsyncReadCompatExt;

use crate::common::cli;

#[tokio::test]
async fn forest_cli_car_concat() -> Result<()> {
    let a = tempfile::Builder::new()
        .prefix("forest-cli-car-concat-a")
        .suffix(".car")
        .tempfile()?;
    new_car(1024, a.path()).await?;

    let b = tempfile::Builder::new()
        .prefix("forest-cli-car-concat-b")
        .suffix(".car")
        .tempfile()?;
    new_car(2048, b.path()).await?;

    let output = tempfile::Builder::new()
        .prefix("forest-cli-car-concat-output")
        .suffix(".car")
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
        .suffix(".car")
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
        .suffix(".car")
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
    let (cid, _data) = new_block(&mut rng.clone());
    let header = CarHeader::from(vec![cid]);

    let mut block_stream = Box::pin(
        futures::stream::unfold(rng, |mut rng| async { Some((new_block(&mut rng), rng)) })
            .take(size),
    );

    let mut writer = tokio::fs::File::create(path).await?.compat();
    header
        .write_stream_async(&mut writer, &mut block_stream)
        .await?;

    Ok(())
}

fn new_block(rng: &mut SmallRng) -> (Cid, Vec<u8>) {
    let mut data = [0; 64];
    rng.fill(&mut data);
    let cid = Cid::new_v1(DAG_CBOR, multihash::Code::Blake2b256.digest(&data));
    (cid, data.to_vec())
}

async fn validate_car(path: impl AsRef<Path>) -> Result<()> {
    let mut reader = CarReader::new(tokio::fs::File::open(path).await?.compat()).await?;
    assert!(reader.validate);
    let mut count = 0;
    while reader.next_block().await?.is_some() {
        count += 1;
    }
    println!("Result car block count: {count}");
    Ok(())
}
