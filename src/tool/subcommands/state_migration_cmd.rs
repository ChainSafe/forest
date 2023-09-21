// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::networks::{ActorBundleInfo, ACTOR_BUNDLES};
use crate::utils::db::car_stream::{CarStream, CarWriter};
use crate::utils::net::global_http_client;
use anyhow::ensure;
use async_compression::tokio::write::ZstdEncoder;
use futures::{stream, StreamExt, TryStreamExt};
use itertools::Itertools as _;
use std::io::{self, Cursor};
use std::path::PathBuf;
use tokio::fs::File;
use tracing::info;

#[derive(Debug, clap::Subcommand)]
pub enum StateMigrationCommands {
    /// Generate a merged actor bundle from the hard-coded sources in forest
    ActorBundle {
        #[arg(default_value = "actor_bundles.car.zst")]
        output: PathBuf,
    },
}

impl StateMigrationCommands {
    pub async fn run(self) -> anyhow::Result<()> {
        match self {
            Self::ActorBundle { output } => generate_actor_bundle(output).await,
        }
    }
}

async fn generate_actor_bundle(output: PathBuf) -> anyhow::Result<()> {
    let (mut roots, blocks) = stream::iter(ACTOR_BUNDLES.iter())
        .then(
            |ActorBundleInfo {
                 manifest: root,
                 url,
             }| async move {
                info!(%url, "downloading bundle");
                let response = global_http_client()
                    .get(url.clone())
                    .send()
                    .await?
                    .error_for_status()?;
                let bytes = response.bytes().await?;
                let car = CarStream::new(Cursor::new(bytes)).await?;
                ensure!(car.header.version == 1);
                ensure!(car.header.roots.len() == 1);
                ensure!(&car.header.roots[0] == root);
                anyhow::Ok((*root, car.try_collect::<Vec<_>>().await?))
            },
        )
        .try_collect::<Vec<_>>()
        .await?
        .into_iter()
        .unzip::<_, _, Vec<_>, Vec<_>>();

    ensure!(roots.iter().all_unique());

    roots.sort(); // deterministic

    let mut blocks = blocks.into_iter().flatten().collect::<Vec<_>>();
    blocks.sort();
    blocks.dedup();

    for block in blocks.iter() {
        ensure!(
            block.valid(),
            "sources contain an invalid block, cid {}",
            block.cid
        )
    }

    stream::iter(blocks)
        .map(io::Result::Ok)
        .forward(CarWriter::new_carv1(
            roots.into_iter().collect(),
            ZstdEncoder::with_quality(
                File::create(output).await?,
                async_compression::Level::Precise(17),
            ),
        )?)
        .await?;

    Ok(())
}

/// If this test fails locally, check you've got `git-lfs` installed and working
#[test]
fn asset_integrity() {
    futures::executor::block_on(async {
        CarStream::new(Cursor::new(include_bytes!(
            "../../../assets/actor_bundles.car.zst"
        )))
        .await
        .unwrap()
        .try_collect::<Vec<_>>()
        .await
        .unwrap();
    });
}
