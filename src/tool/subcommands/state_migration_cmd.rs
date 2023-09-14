// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::cid_collections::{hash_map::Entry, CidHashMap, CidHashSet};
use crate::networks::{ActorBundleInfo, ACTOR_BUNDLES};
use crate::utils::db::car_stream::{CarBlock, CarStream, CarWriter};
use crate::utils::net::global_http_client;
use anyhow::{bail, ensure};
use cid::Cid;
use futures::{stream, StreamExt as _, TryStreamExt as _};
use std::io::Cursor;
use std::path::PathBuf;
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
    let (roots, blocks) = stream::iter(ACTOR_BUNDLES.iter())
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
                anyhow::Ok((root, car.try_collect::<Vec<_>>().await?))
            },
        )
        .try_collect::<Vec<_>>()
        .await?
        .into_iter()
        .unzip::<_, _, Vec<&Cid>, Vec<_>>();

    let roots = roots.into_iter().cloned().collect::<CidHashSet>();
    let blocks = blocks.into_iter().flatten().try_fold(
        CidHashMap::new(),
        |mut acc, CarBlock { cid, data }| {
            // TODO(aatifsyed): check cid matches data?
            match acc.entry(cid) {
                Entry::Vacant(v) => {
                    v.insert(data);
                }
                Entry::Occupied(o) if o.get() == &data => {}
                Entry::Occupied(_) => {
                    bail!("clobbered data for block with cid {cid}")
                }
            };
            anyhow::Ok(acc)
        },
    )?;

    let mut car = vec![];

    stream::iter(blocks)
        .map(|(cid, data)| std::io::Result::Ok(CarBlock { cid, data }))
        .forward(CarWriter::new_carv1(roots.into_iter().collect(), &mut car)?)
        .await?;

    let car_zst = zstd::encode_all(car.as_slice(), 17)?;

    tokio::fs::write(output, car_zst).await?;
    Ok(())
}

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
