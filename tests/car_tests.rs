// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod common;

use std::path::Path;

use anyhow::*;
use cid::{
    multihash::{self, MultihashDigest},
    Cid,
};
use fvm_ipld_car::{CarHeader, CarReader};
use fvm_ipld_encoding::DAG_CBOR;
use rand::{rngs::OsRng, Rng};
use tempfile::NamedTempFile;
use tokio_util::compat::TokioAsyncReadCompatExt;

use crate::common::cli;

#[tokio::test]
async fn forest_cli_car_concat() -> Result<()> {
    let a = NamedTempFile::new()?;
    new_car(1024, a.path()).await?;
    let b = NamedTempFile::new()?;
    new_car(2048, b.path()).await?;
    let output = NamedTempFile::new()?;

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
    let output = NamedTempFile::new()?;

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

async fn new_car(size: usize, path: impl AsRef<Path>) -> Result<()> {
    let (tx, rx) = flume::bounded(100);
    let (cid, data) = new_block();
    let header = CarHeader::from(vec![cid]);
    tx.send((cid, data))?;

    let mut writer = tokio::fs::File::create(path).await?.compat();
    let write_task = tokio::spawn(async move {
        let mut stream = rx.stream();
        header.write_stream_async(&mut writer, &mut stream).await?;
        Ok(())
    });

    for _ in 1..size {
        tx.send_async(new_block()).await?;
    }

    drop(tx);
    write_task.await??;

    Ok(())
}

fn new_block() -> (Cid, Vec<u8>) {
    let mut data = [0; 1024];
    OsRng.fill(&mut data);
    let cid = Cid::new_v1(DAG_CBOR, multihash::Code::Blake2b256.digest(&data));
    (cid, data.to_vec())
}

async fn validate_car(path: impl AsRef<Path>) -> Result<()> {
    let mut reader = CarReader::new(tokio::fs::File::open(path).await?.compat()).await?;
    assert!(reader.validate);
    while reader.next_block().await?.is_some() {}
    Ok(())
}
