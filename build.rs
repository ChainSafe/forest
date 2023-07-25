// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{
    env, io,
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
use tokio::task::JoinSet;
use walkdir::WalkDir;

const PROTO_DIR: &str = "proto";
const CARGO_OUT_DIR: &str = "proto";

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
    println!("cargo:rerun-if-changed=actor_bundles/*.car");

    let mut tasks = JoinSet::new();
    for (root, url) in [
        // calibnet
        (
            Cid::try_from("bafy2bzacedbedgynklc4dgpyxippkxmba2mgtw7ecntoneclsvvl4klqwuyyy").unwrap(),
            "https://forest-continuous-integration.fra1.cdn.digitaloceanspaces.com/builtin-actors/calibnet/Shark.car",
        ),
        (
            Cid::try_from("bafy2bzaced25ta3j6ygs34roprilbtb3f6mxifyfnm7z7ndquaruxzdq3y7lo").unwrap(),
            "https://forest-continuous-integration.fra1.cdn.digitaloceanspaces.com/builtin-actors/calibnet/Hygge.car",
        ),
        (
            Cid::try_from("bafy2bzacedhuowetjy2h4cxnijz2l64h4mzpk5m256oywp4evarpono3cjhco").unwrap(),
            "https://forest-continuous-integration.fra1.cdn.digitaloceanspaces.com/builtin-actors/calibnet/Lightning.car",
        ),
        // devnet
        (
            Cid::try_from("bafy2bzacebzz376j5kizfck56366kdz5aut6ktqrvqbi3efa2d4l2o2m653ts").unwrap(),
            "https://forest-continuous-integration.fra1.cdn.digitaloceanspaces.com/builtin-actors/devnet/Hygge.car",
        ),
        (
            Cid::try_from("bafy2bzaceay35go4xbjb45km6o46e5bib3bi46panhovcbedrynzwmm3drr4i").unwrap(),
            "https://forest-continuous-integration.fra1.cdn.digitaloceanspaces.com/builtin-actors/devnet/Lightning.car",
        ),
        // mainnet
        (
            Cid::try_from("bafy2bzaceb6j6666h36xnhksu3ww4kxb6e25niayfgkdnifaqi6m6ooc66i6i").unwrap(),
            "https://forest-continuous-integration.fra1.cdn.digitaloceanspaces.com/builtin-actors/mainnet/Shark.car",
        ),
        (
            Cid::try_from("bafy2bzacecsuyf7mmvrhkx2evng5gnz5canlnz2fdlzu2lvcgptiq2pzuovos").unwrap(),
            "https://forest-continuous-integration.fra1.cdn.digitaloceanspaces.com/builtin-actors/mainnet/Hygge.car",
        ),
        (
            Cid::try_from("bafy2bzacecnhaiwcrpyjvzl4uv4q3jzoif26okl3m66q3cijp3dfwlcxwztwo").unwrap(),
            "https://forest-continuous-integration.fra1.cdn.digitaloceanspaces.com/builtin-actors/mainnet/Lightning.car",
        ),
    ] {
        tasks.spawn(async move {
            download_bundle_if_needed(root, url).await.with_context(|| format!("Failed to get {root}.car from {url}"))
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

async fn download_bundle_if_needed(root: Cid, url: &str) -> anyhow::Result<PathBuf> {
    const ACTOR_BUNDLE_CACHE_DIR: &str = "./actor_bundles/";
    // Using a local path instead of `OUT_DIR` to reuse the cache as much as possible
    let cached_path = Path::new(ACTOR_BUNDLE_CACHE_DIR).join(format!("{root}.car"));
    if cached_path.is_file() {
        if let Ok(file) = async_fs::File::open(&cached_path).await {
            if let Ok(true) = is_bundle_valid(&root, BufReader::new(file)).await {
                return Ok(cached_path);
            }
        }
    }

    let tmp = tempfile::NamedTempFile::new_in(ACTOR_BUNDLE_CACHE_DIR)?.into_temp_path();
    {
        let response = global_http_client().get(url).send().await?;
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
    if is_bundle_valid(&root, BufReader::new(async_fs::File::open(&tmp).await?)).await? {
        tmp.persist(&cached_path)?;
        Ok(cached_path)
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
