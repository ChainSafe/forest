// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::io::{self, Cursor};
use std::path::Path;
use std::sync::LazyLock;

use ahash::HashMap;
use anyhow::ensure;
use async_compression::tokio::write::ZstdEncoder;
use cid::Cid;
use futures::stream::FuturesUnordered;
use futures::{StreamExt, TryStreamExt, stream};
use fvm_ipld_blockstore::MemoryBlockstore;
use itertools::Itertools;
use nunny::Vec as NonEmpty;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use serde_with::{DisplayFromStr, serde_as};
use tokio::fs::File;
use tracing::warn;

use crate::daemon::bundle::{ACTOR_BUNDLE_CACHE_DIR, load_actor_bundles_from_server};
use crate::shim::machine::BuiltinActorManifest;
use crate::utils::db::car_stream::{CarStream, CarWriter};
use crate::utils::net::{DownloadFileOption, download_file_with_cache};

use std::str::FromStr;

use super::NetworkChain;

#[derive(Debug)]
pub struct ActorBundleInfo {
    pub manifest: Cid,
    pub url: Url,
    /// Alternative URL to download the bundle from if the primary URL fails.
    /// Note that we host the bundles and so we need to update the bucket
    /// ourselves when a new bundle is released.
    pub alt_url: Url,
    pub network: NetworkChain,
    pub version: String,
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
                          "https://filecoin-actors.chainsafe.dev/",
                          $version,
                            "/builtin-actors-",
                            $network,
                            ".car"
                        ).parse().unwrap(),
                    network: NetworkChain::from_str($network).unwrap(),
                    version: $version.to_string(),
                },
            )*
        ]
    }
}

pub static ACTOR_BUNDLES: LazyLock<Box<[ActorBundleInfo]>> = LazyLock::new(|| {
    Box::new(actor_bundle_info![
        "bafy2bzacedrdn6z3z7xz7lx4wll3tlgktirhllzqxb766dxpaqp3ukxsjfsba" @ "8.0.0-rc.1" for "calibrationnet",
        "bafy2bzacedbedgynklc4dgpyxippkxmba2mgtw7ecntoneclsvvl4klqwuyyy" @ "v9.0.3" for "calibrationnet",
        "bafy2bzaced25ta3j6ygs34roprilbtb3f6mxifyfnm7z7ndquaruxzdq3y7lo" @ "v10.0.0-rc.1" for "calibrationnet",
        "bafy2bzacedhuowetjy2h4cxnijz2l64h4mzpk5m256oywp4evarpono3cjhco" @ "v11.0.0-rc2" for "calibrationnet",
        "bafy2bzacedrunxfqta5skb7q7x32lnp4efz2oq7fn226ffm7fu5iqs62jkmvs" @ "v12.0.0-rc.1" for "calibrationnet",
        "bafy2bzacebl4w5ptfvuw6746w7ev562idkbf5ppq72e6zub22435ws2rukzru" @ "v12.0.0-rc.2" for "calibrationnet",
        "bafy2bzacednzb3pkrfnbfhmoqtb3bc6dgvxszpqklf3qcc7qzcage4ewzxsca" @ "v12.0.0" for "calibrationnet",
        "bafy2bzacea4firkyvt2zzdwqjrws5pyeluaesh6uaid246tommayr4337xpmi" @ "v13.0.0-rc.3" for "calibrationnet",
        "bafy2bzacect4ktyujrwp6mjlsitnpvuw2pbuppz6w52sfljyo4agjevzm75qs" @ "v13.0.0" for "calibrationnet",
        "bafy2bzacebq3hncszqpojglh2dkwekybq4zn6qpc4gceqbx36wndps5qehtau" @ "v14.0.0-rc.1" for "calibrationnet",
        "bafy2bzaceax5zkysst7vtyup4whdxwzlpnaya3qp34rnoi6gyt4pongps7obw" @ "v15.0.0" for "calibrationnet",
        "bafy2bzacebc7zpsrihpyd2jdcvmegbbk6yhzkifre3hxtoul5wdxxklbwitry" @ "v16.0.0-rc3" for "calibrationnet",
        "bafy2bzacecqtwq6hjhj2zy5gwjp76a4tpcg2lt7dps5ycenvynk2ijqqyo65e" @ "v16.0.1" for "calibrationnet",
        "bafy2bzacecn64rlb52rjsvgopnidz6w42z3zobmjxqek5s4xqjh3ly47rcurg" @ "v17.0.0" for "calibrationnet",
        "bafy2bzacearjal5rsmzloz3ny7aoju2rgw66wgxdrydgg27thcsazbmf5qihq" @ "v15.0.0-rc1" for "butterflynet",
        "bafy2bzaceda5lc7qrwp2hdm6s6erppwuydsfqrhbgld7juixalk342inqimbo" @ "v16.0.1" for "butterflynet",
        "bafy2bzacedzjwguwuihh4tptzfkkwaj3naamrnklbaixn2wfzqh67twwp56pi" @ "v17.0.0" for "butterflynet",
        "bafy2bzacedozk3jh2j4nobqotkbofodq4chbrabioxbfrygpldgoxs3zwgggk" @ "v9.0.3" for "devnet",
        "bafy2bzacebzz376j5kizfck56366kdz5aut6ktqrvqbi3efa2d4l2o2m653ts" @ "v10.0.0" for "devnet",
        "bafy2bzaceay35go4xbjb45km6o46e5bib3bi46panhovcbedrynzwmm3drr4i" @ "v11.0.0" for "devnet",
        "bafy2bzaceasjdukhhyjbegpli247vbf5h64f7uvxhhebdihuqsj2mwisdwa6o" @ "v12.0.0" for "devnet",
        "bafy2bzacecn7uxgehrqbcs462ktl2h23u23cmduy2etqj6xrd6tkkja56fna4" @ "v13.0.0" for "devnet",
        "bafy2bzacebwn7ymtozv5yz3x5hnxl4bds2grlgsk5kncyxjak3hqyhslb534m" @ "v14.0.0-rc.1" for "devnet",
        "bafy2bzacedlusqjwf7chvl2ve2fum5noyqrtjzcrzkhpbzpkg7puiru7dj4ug" @ "v15.0.0-rc1" for "devnet",
        "bafy2bzaceclp3wfrwdjgh6c3gee5smwj3zmmrhb4fdbc4yfchfaia6rlljx5o" @ "v16.0.1" for "devnet",
        "bafy2bzaceasvgkke3j4cs3xsxnjswpcdmokkvkiehzxzcgfox3ozlehimbuqk" @ "v17.0.0" for "devnet",
        "bafy2bzaceb6j6666h36xnhksu3ww4kxb6e25niayfgkdnifaqi6m6ooc66i6i" @ "v9.0.3" for "mainnet",
        "bafy2bzacecsuyf7mmvrhkx2evng5gnz5canlnz2fdlzu2lvcgptiq2pzuovos" @ "v10.0.0" for "mainnet",
        "bafy2bzacecnhaiwcrpyjvzl4uv4q3jzoif26okl3m66q3cijp3dfwlcxwztwo" @ "v11.0.0" for "mainnet",
        "bafy2bzaceapkgfggvxyllnmuogtwasmsv5qi2qzhc2aybockd6kag2g5lzaio" @ "v12.0.0" for "mainnet",
        "bafy2bzacecdhvfmtirtojwhw2tyciu4jkbpsbk5g53oe24br27oy62sn4dc4e" @ "v13.0.0" for "mainnet",
        "bafy2bzacecbueuzsropvqawsri27owo7isa5gp2qtluhrfsto2qg7wpgxnkba" @ "v14.0.0" for "mainnet",
        "bafy2bzaceakwje2hyinucrhgtsfo44p54iw4g6otbv5ghov65vajhxgntr53u" @ "v15.0.0" for "mainnet",
        "bafy2bzacecnepvsh4lw6pwljobvwm6zwu6mbwveatp7llhpuguvjhjiqz7o46" @ "v16.0.1" for "mainnet",
        "bafy2bzaceai74ppsvuxs3nvpzzeuptdr3wl7vmdpbphvtz4qt5hfq2qdfvz3e" @ "v17.0.0" for "mainnet",
    ])
});

#[serde_as]
#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct ActorBundleMetadata {
    pub network: NetworkChain,
    pub version: String,
    #[serde_as(as = "DisplayFromStr")]
    pub bundle_cid: Cid,
    pub manifest: BuiltinActorManifest,
}

impl ActorBundleMetadata {
    pub fn actor_major_version(&self) -> anyhow::Result<u64> {
        self.version
            .trim_start_matches('v')
            .split('.')
            .next()
            .ok_or_else(|| anyhow::anyhow!("invalid version"))
            .and_then(|s| s.parse().map_err(|_| anyhow::anyhow!("invalid version")))
    }
}

type ActorBundleMetadataMap = HashMap<(NetworkChain, String), ActorBundleMetadata>;

pub static ACTOR_BUNDLES_METADATA: LazyLock<ActorBundleMetadataMap> = LazyLock::new(|| {
    let json: &str = include_str!("../../build/manifest.json");
    let metadata_vec: Vec<ActorBundleMetadata> =
        serde_json::from_str(json).expect("invalid manifest");
    metadata_vec
        .into_iter()
        .map(|metadata| {
            (
                (metadata.network.clone(), metadata.version.clone()),
                metadata,
            )
        })
        .collect()
});

pub async fn get_actor_bundles_metadata() -> anyhow::Result<Vec<ActorBundleMetadata>> {
    let store = MemoryBlockstore::new();
    for network in [
        NetworkChain::Mainnet,
        NetworkChain::Calibnet,
        NetworkChain::Butterflynet,
        NetworkChain::Devnet(Default::default()),
    ] {
        load_actor_bundles_from_server(&store, &network, &ACTOR_BUNDLES).await?;
    }

    ACTOR_BUNDLES
        .iter()
        .map(|bundle| -> anyhow::Result<_> {
            Ok(ActorBundleMetadata {
                network: bundle.network.clone(),
                version: bundle.version.clone(),
                bundle_cid: bundle.manifest,
                manifest: BuiltinActorManifest::load_manifest(&store, &bundle.manifest)?,
            })
        })
        .collect()
}

pub async fn generate_actor_bundle(output: &Path) -> anyhow::Result<()> {
    let (mut roots, blocks) = FuturesUnordered::from_iter(ACTOR_BUNDLES.iter().map(
        |ActorBundleInfo {
             manifest: root,
             url,
             alt_url,
             network,
             version,
         }| async move {
            let result = if let Ok(response) =
                download_file_with_cache(url, &ACTOR_BUNDLE_CACHE_DIR, DownloadFileOption::NonResumable).await
            {
                response
            } else {
                warn!(
                    "failed to download bundle {network}-{version} from primary URL, trying alternative URL"
                );
                download_file_with_cache(alt_url, &ACTOR_BUNDLE_CACHE_DIR, DownloadFileOption::NonResumable).await?
            };

            let bytes = std::fs::read(&result.path)?;
            let car = CarStream::new(Cursor::new(bytes)).await?;
            ensure!(car.header_v1.roots.len() == 1);
            ensure!(car.header_v1.roots.first() == root);
            anyhow::Ok((*root, car.try_collect::<Vec<_>>().await?))
        },
    ))
    .try_collect::<Vec<_>>()
    .await?
    .into_iter()
    .unzip::<_, _, Vec<_>, Vec<_>>();

    ensure!(roots.iter().all_unique());

    roots.sort(); // deterministic

    let mut blocks = blocks.into_iter().flatten().collect_vec();
    blocks.sort();
    blocks.dedup();

    for block in blocks.iter() {
        block.validate()?;
    }

    stream::iter(blocks)
        .map(io::Result::Ok)
        .forward(CarWriter::new_carv1(
            NonEmpty::new(roots).map_err(|_| anyhow::Error::msg("car roots cannot be empty"))?,
            ZstdEncoder::with_quality(
                File::create(&output).await?,
                async_compression::Level::Precise(17),
            ),
        )?)
        .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use http::StatusCode;
    use reqwest::Response;
    use std::time::Duration;

    use crate::utils::net::global_http_client;

    use super::*;

    #[tokio::test]
    async fn check_bundles_are_mirrored() {
        // Run the test only in CI so that regular test on dev machines don't download the bundles
        // on poor internet connections.
        if std::env::var("CI").is_err() {
            return;
        }

        FuturesUnordered::from_iter(ACTOR_BUNDLES.iter().map(
            |ActorBundleInfo {
                 manifest,
                 url,
                 alt_url,
                 network: _,
                 version: _,
             }| async move {
                let (primary, alt) = match (http_get(url).await, http_get(alt_url).await) {
                    (Ok(primary), Ok(alt)) => (primary, alt),
                    (Err(_), Err(_)) => anyhow::bail!("Both sources are down"),
                    // If either of the sources are otherwise down, we don't want to fail the test.
                    _ => return anyhow::Ok(()),
                };

                // Check that neither of the sources respond with 404.
                // Such code would indicate that the bundle URLs are incorrect.
                // In case of GH releases, it may have been yanked for some reason.
                // In case of our own bundles, it may have been not uploaded (or deleted).
                assert_ne!(
                    StatusCode::NOT_FOUND,
                    primary.status(),
                    "Could not download {url}"
                );
                assert_ne!(
                    StatusCode::NOT_FOUND,
                    alt.status(),
                    "Could not download {alt_url}"
                );

                // If either of the sources are otherwise down, we don't want to fail the test.
                // This is because we don't want to fail the test if the infrastructure is down.
                if !primary.status().is_success() || !alt.status().is_success() {
                    return anyhow::Ok(());
                }

                // Check that the bundles are identical.
                // This is to ensure that the bundle was not tamperered with and that the
                // bundle was uploaded to the alternative URL correctly.
                let (primary, alt) = match (primary.bytes().await, alt.bytes().await) {
                    (Ok(primary), Ok(alt)) => (primary, alt),
                    (Err(_), Err(_)) => anyhow::bail!("Both sources are down"),
                    // If either of the sources are otherwise down, we don't want to fail the test.
                    _ => return anyhow::Ok(()),
                };

                let car_primary = CarStream::new(Cursor::new(primary)).await?;
                let car_secondary = CarStream::new(Cursor::new(alt)).await?;

                assert_eq!(
                    car_primary.header_v1.roots, car_secondary.header_v1.roots,
                    "Roots for {url} and {alt_url} do not match"
                );
                assert_eq!(
                    car_primary.header_v1.roots.first(),
                    manifest,
                    "Manifest for {url} and {alt_url} does not match"
                );

                Ok(())
            },
        ))
        .try_collect::<Vec<_>>()
        .await
        .unwrap();
    }

    pub async fn http_get(url: &Url) -> anyhow::Result<Response> {
        Ok(global_http_client()
            .get(url.clone())
            .timeout(Duration::from_secs(120))
            .send()
            .await?)
    }

    #[test]
    fn test_actor_major_version_correct() {
        let cases = [
            ("8.0.0-rc.1", 8),
            ("v9.0.3", 9),
            ("v10.0.0-rc.1", 10),
            ("v12.0.0", 12),
            ("v13.0.0-rc.3", 13),
            ("v13.0.0", 13),
            ("v14.0.0-rc.1", 14),
        ];

        for (version, expected) in cases.iter() {
            let metadata = ActorBundleMetadata {
                network: NetworkChain::Mainnet,
                version: version.to_string(),
                bundle_cid: Default::default(),
                manifest: Default::default(),
            };

            assert_eq!(metadata.actor_major_version().unwrap(), *expected);
        }
    }

    #[test]
    fn test_actor_major_version_invalid() {
        let cases = ["cthulhu", "vscode", ".02", "-42"];

        for version in cases.iter() {
            let metadata = ActorBundleMetadata {
                network: NetworkChain::Mainnet,
                version: version.to_string(),
                bundle_cid: Default::default(),
                manifest: Default::default(),
            };

            assert!(metadata.actor_major_version().is_err());
        }
    }
}
