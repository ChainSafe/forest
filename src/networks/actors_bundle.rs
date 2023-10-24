// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::io::{self, Cursor};
use std::path::Path;

use anyhow::ensure;
use async_compression::tokio::write::ZstdEncoder;
use cid::Cid;
use futures::{stream, StreamExt, TryStreamExt};
use itertools::Itertools;
use once_cell::sync::Lazy;
use reqwest::Url;
use tokio::fs::File;
use tracing::warn;

use crate::utils::db::car_stream::{CarStream, CarWriter};
use crate::utils::net::http_get;

#[derive(Debug)]
pub struct ActorBundleInfo {
    pub manifest: Cid,
    pub url: Url,
    /// Alternative URL to download the bundle from if the primary URL fails.
    /// Note that we host the bundles and so we need to update the bucket
    /// ourselves when a new bundle is released.
    pub alt_url: Url,
}

macro_rules! actor_bundle_info {
    ($($cid:literal @ $version:literal for $network:literal),* $(,)?) => {
        [
            $(
                ActorBundleInfo {
                    manifest: $cid.parse().unwrap(),
                    url: concat!(
                            "https://github.com/filecoin-project/builtin-actors/releases/download/",
                            $version,
                            "/builtin-actors-",
                            $network,
                            ".car"
                        ).parse().unwrap(),
                    alt_url: concat!(
                          "https://forest-snapshots.fra1.cdn.digitaloceanspaces.com/actors/",
                          $version,
                            "/builtin-actors-",
                            $network,
                            ".car"
                        ).parse().unwrap()
                },
            )*
        ]
    }
}

pub static ACTOR_BUNDLES: Lazy<Box<[ActorBundleInfo]>> = Lazy::new(|| {
    Box::new(actor_bundle_info![
        "bafy2bzacedbedgynklc4dgpyxippkxmba2mgtw7ecntoneclsvvl4klqwuyyy" @ "v9.0.3" for "calibrationnet",
        "bafy2bzaced25ta3j6ygs34roprilbtb3f6mxifyfnm7z7ndquaruxzdq3y7lo" @ "v10.0.0-rc.1" for "calibrationnet",
        "bafy2bzacedhuowetjy2h4cxnijz2l64h4mzpk5m256oywp4evarpono3cjhco" @ "v11.0.0-rc2" for "calibrationnet",
        "bafy2bzacedrunxfqta5skb7q7x32lnp4efz2oq7fn226ffm7fu5iqs62jkmvs" @ "v12.0.0-rc.1" for "calibrationnet",
        "bafy2bzacedozk3jh2j4nobqotkbofodq4chbrabioxbfrygpldgoxs3zwgggk" @ "v9.0.3" for "devnet",
        "bafy2bzacebzz376j5kizfck56366kdz5aut6ktqrvqbi3efa2d4l2o2m653ts" @ "v10.0.0" for "devnet",
        "bafy2bzaceay35go4xbjb45km6o46e5bib3bi46panhovcbedrynzwmm3drr4i" @ "v11.0.0" for "devnet",
        "bafy2bzacebk6yiirh4ennphzyka7b6g6jzn3lt4lr5ht7rjwulnrcthjihapo" @ "v12.0.0-rc.1" for "devnet",
        "bafy2bzaceb6j6666h36xnhksu3ww4kxb6e25niayfgkdnifaqi6m6ooc66i6i" @ "v9.0.3" for "mainnet",
        "bafy2bzacecsuyf7mmvrhkx2evng5gnz5canlnz2fdlzu2lvcgptiq2pzuovos" @ "v10.0.0" for "mainnet",
        "bafy2bzacecnhaiwcrpyjvzl4uv4q3jzoif26okl3m66q3cijp3dfwlcxwztwo" @ "v11.0.0" for "mainnet",
    ])
});

pub async fn generate_actor_bundle(output: &Path) -> anyhow::Result<()> {
    let (mut roots, blocks) = stream::iter(ACTOR_BUNDLES.iter())
        .then(
            |ActorBundleInfo {
                 manifest: root,
                 url,
                 alt_url,
             }| async move {
                let response = if let Ok(response) = http_get(url).await {
                    response
                } else {
                    warn!("failed to download bundle from primary URL, trying alternative URL");
                    http_get(alt_url).await?
                };
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
                File::create(&output).await?,
                async_compression::Level::Precise(17),
            ),
        )?)
        .await?;

    Ok(())
}
