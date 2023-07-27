// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod src;
use src::{ActorBundleInfo, ACTOR_BUNDLES};

use std::{
    env, fs, io,
    path::{Path, PathBuf},
    pin::pin,
};

use anyhow::Context;
use async_compression::futures::write::ZstdEncoder;
use cid::Cid;
use futures::{
    io::{BufReader, BufWriter},
    AsyncRead, AsyncWriteExt, Stream, StreamExt, TryStreamExt,
};
use fvm_ipld_car::{CarHeader, CarReader};
use itertools::Itertools;
use once_cell::sync::Lazy;
use protobuf_codegen::Customize;
use reqwest::Url;
use tokio::task::JoinSet;
use walkdir::WalkDir;

const PROTO_DIR: &str = "proto";
const CARGO_OUT_DIR: &str = "proto";

// Using a local path instead of `OUT_DIR` to reuse the cache as much as possible
static ACTOR_BUNDLE_CACHE_DIR: Lazy<PathBuf> =
    Lazy::new(|| Path::new("target/actor_bundles/").to_owned());

pub fn global_http_client() -> reqwest::Client {
    static CLIENT: Lazy<reqwest::Client> = Lazy::new(reqwest::Client::new);
    CLIENT.clone()
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    generate_protobuf_code()?;

    generate_compressed_actor_bundles().await?;

    Ok(())
}

async fn generate_compressed_actor_bundles() -> anyhow::Result<()> {
    println!("cargo:rerun-if-changed=src/mod.rs");
    println!(
        "cargo:rerun-if-changed={}",
        ACTOR_BUNDLE_CACHE_DIR.display()
    );

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

fn read_car_as_stream<R>(reader: CarReader<R>) -> impl Stream<Item = (Cid, Vec<u8>)>
where
    R: AsyncRead + Send + Unpin,
{
    futures::stream::unfold(reader, move |mut reader| async {
        reader
            .next_block()
            .await
            .expect("Failed to call CarReader::next_block")
            .map(|b| ((b.cid, b.data), reader))
    })
}

fn merge_car_readers<R>(readers: Vec<CarReader<R>>) -> impl Stream<Item = (Cid, Vec<u8>)>
where
    R: AsyncRead + Send + Unpin,
{
    futures::stream::iter(readers).flat_map(read_car_as_stream)
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

fn generate_protobuf_code() -> anyhow::Result<()> {
    println!("cargo:rerun-if-changed=proto");

    protobuf_codegen::Codegen::new()
        .pure()
        .cargo_out_dir(CARGO_OUT_DIR)
        .inputs(get_proto_inputs()?.as_slice())
        .include(PROTO_DIR)
        .customize(Customize::default().lite_runtime(true))
        .run()?;
    Ok(())
}

fn get_proto_inputs() -> anyhow::Result<Vec<PathBuf>> {
    let mut inputs = Vec::new();
    for entry in WalkDir::new(PROTO_DIR).into_iter().flatten() {
        let path = entry.path();
        if path.is_file() {
            if let Some(ext) = path.extension() {
                if ext == "proto" {
                    inputs.push(path.into());
                }
            }
        }
    }
    Ok(inputs)
}
