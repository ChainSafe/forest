// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::build::{merge_car_readers, ActorBundleInfo, ACTOR_BUNDLES};
use crate::utils::net::global_http_client;
use anyhow::{Context as _, Result};
use async_compression::futures::write::ZstdEncoder;
use cid::Cid;
use clap::Subcommand;
use futures::io::{BufReader, BufWriter};
use futures::{AsyncRead, AsyncWriteExt, TryStreamExt};
use fvm_ipld_car::{CarHeader, CarReader};
use itertools::Itertools;
use once_cell::sync::Lazy;
use reqwest::Url;
use std::env;
use std::path::{Path, PathBuf};
use std::pin::pin;
use std::{fs, io};
use tokio::task::JoinSet;

static ACTOR_BUNDLE_CACHE_DIR: Lazy<PathBuf> =
    Lazy::new(|| env::temp_dir().join(".forest_actor_bundles/"));

#[derive(Debug, Subcommand)]
pub enum StateMigrationCommands {
    /// Generate a merged actor bundle
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
    let mut tasks = JoinSet::new();
    for ActorBundleInfo { manifest, url } in ACTOR_BUNDLES.iter() {
        tasks.spawn(async move {
            download_bundle_if_needed(manifest, url)
                .await
                .with_context(|| format!("Failed to get {manifest}.car from {url}"))
        });
    }

    let mut car_roots = vec![];
    let mut car_readers = vec![];
    while let Some(path) = tasks.join_next().await {
        let car_reader = fvm_ipld_car::CarReader::new(futures::io::BufReader::new(
            async_fs::File::open(path??).await?,
        ))
        .await?;
        car_roots.extend_from_slice(car_reader.header.roots.as_slice());
        car_readers.push(car_reader);
    }

    let car_writer = CarHeader::from(
        car_readers
            .iter()
            .flat_map(|it| it.header.roots.iter())
            .unique()
            .cloned()
            .collect::<Vec<_>>(),
    );

    let mut zstd_encoder = ZstdEncoder::with_quality(
        async_fs::File::create(Path::new(&env::var("OUT_DIR")?).join("actor_bundles.car.zst"))
            .await?,
        if cfg!(debug_assertions) {
            async_compression::Level::Default
        } else {
            async_compression::Level::Precise(17)
        },
    );

    car_writer
        .write_stream_async(&mut zstd_encoder, &mut pin!(merge_car_readers(car_readers)))
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
