// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::networks::{ActorBundleInfo, ACTOR_BUNDLES};
use crate::utils::db::car_stream::CarStream;
use crate::utils::db::car_stream::CarWriter;
use crate::utils::db::car_util::merge_car_streams;
use crate::utils::net::global_http_client;
use anyhow::{Context as _, Result};
use async_compression::tokio::write::ZstdEncoder;
use cid::Cid;
use clap::Subcommand;
use futures::io::{BufReader, BufWriter};
use futures::{AsyncRead, AsyncWriteExt, StreamExt, TryStreamExt};
use fvm_ipld_car::CarReader;
use itertools::Itertools;
use once_cell::sync::Lazy;
use reqwest::Url;
use std::env;
use std::path::{Path, PathBuf};
use std::{fs, io};

const DEFAULT_BUNDLE_FILE_NAME: &str = "actor_bundles.car.zst";

static ACTOR_BUNDLE_CACHE_DIR: Lazy<PathBuf> =
    Lazy::new(|| env::temp_dir().join(".forest_actor_bundles/"));

#[derive(Debug, Subcommand)]
pub enum StateMigrationCommands {
    /// Generate a merged actor bundle in `.car.zst` format under the current working directory.
    ActorBundle,
}

impl StateMigrationCommands {
    pub async fn run(self) -> Result<()> {
        use StateMigrationCommands::*;

        match self {
            ActorBundle => generate_actor_bundle().await,
        }
    }
}

async fn generate_actor_bundle() -> Result<()> {
    let mut tasks = Vec::with_capacity(ACTOR_BUNDLES.len());
    for ActorBundleInfo { manifest, url } in ACTOR_BUNDLES.iter() {
        tasks.push(tokio::spawn(async move {
            download_bundle_if_needed(manifest, url)
                .await
                .with_context(|| format!("Failed to get {manifest}.car from {url}"))
        }));
    }

    let mut car_streams = vec![];
    for task in tasks {
        let path = task.await??;
        let car_stream = CarStream::new(tokio::io::BufReader::new(
            tokio::fs::File::open(path).await?,
        ))
        .await?;
        car_streams.push(car_stream);
    }

    let all_roots = car_streams
        .iter()
        .flat_map(|it| it.header.roots.iter())
        .unique()
        .cloned()
        .collect::<Vec<_>>();

    let zstd_encoder = ZstdEncoder::with_quality(
        tokio::fs::File::create(Path::new(DEFAULT_BUNDLE_FILE_NAME)).await?,
        async_compression::Level::Precise(zstd::zstd_safe::max_c_level()),
    );

    let stream =
        merge_car_streams(car_streams).map(|b| b.expect("There should be no invalid blocks"));

    stream
        .map(Ok)
        .forward(CarWriter::new_carv1(all_roots, zstd_encoder)?)
        .await?;
    Ok(())
}

async fn download_bundle_if_needed(root: &Cid, url: &Url) -> anyhow::Result<PathBuf> {
    if !ACTOR_BUNDLE_CACHE_DIR.is_dir() {
        fs::create_dir_all(ACTOR_BUNDLE_CACHE_DIR.as_path())?;
    }
    let cached_car_path = ACTOR_BUNDLE_CACHE_DIR.join(format!("{root}.car"));
    if cached_car_path.is_file() {
        if let Ok(file) = async_fs::File::open(&cached_car_path).await {
            if let Ok(true) = is_bundle_valid(root, BufReader::new(file)).await {
                return Ok(cached_car_path);
            }
        }
    }

    let tmp = tempfile::NamedTempFile::new_in(ACTOR_BUNDLE_CACHE_DIR.as_path())?.into_temp_path();
    {
        let response = global_http_client().get(url.clone()).send().await?;
        let mut writer = BufWriter::new(async_fs::File::create(&tmp).await?);
        futures::io::copy(
            response
                .bytes_stream()
                .map_err(|reqwest_error| io::Error::new(io::ErrorKind::Other, reqwest_error))
                .into_async_read(),
            &mut writer,
        )
        .await?;
        writer.flush().await?;
        writer.close().await?;
    }
    if is_bundle_valid(root, BufReader::new(async_fs::File::open(&tmp).await?)).await? {
        tmp.persist(&cached_car_path)?;
        Ok(cached_car_path)
    } else {
        anyhow::bail!("Invalid bundle: {url}");
    }
}

async fn is_bundle_valid<R>(root: &Cid, reader: R) -> anyhow::Result<bool>
where
    R: AsyncRead + Send + Unpin,
{
    is_bundle_car_valid(root, CarReader::new(reader).await?)
}

fn is_bundle_car_valid<R>(root: &Cid, car_reader: CarReader<R>) -> anyhow::Result<bool>
where
    R: AsyncRead + Send + Unpin,
{
    Ok(car_reader.header.roots.len() == 1 && &car_reader.header.roots[0] == root)
}
