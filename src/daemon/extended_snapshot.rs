// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Download and import of the extended snapshots (receipts/events and tipset lookup)
//! published next to a snapshot in the Forest archive.

use crate::blocks::{Tipset, TipsetKey};
use crate::chain::{TIPSET_LOOKUP_HAMT_BIT_WIDTH, index::ChainIndex};
use crate::daemon::db_util::path_as_url;
use crate::db::car::forest::new_forest_car_temp_path_in;
use crate::db::{EthMappingsStore, MemoryDB};
use crate::prelude::*;
use crate::shim::clock::ChainEpoch;
use crate::utils::db::car_stream::{CarBlock, CarStream};
use crate::utils::net::{DownloadFileOption, download_to};
use anyhow::Context;
use fil_actors_shared::fvm_ipld_hamt::Hamt;
use futures::TryStreamExt;
use regex::Regex;
use serde::Deserialize;
use std::path::Path;
use std::sync::{Arc, LazyLock};
use tracing::{info, warn};
use url::Url;

static FILENAME_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Za-z0-9_.-]+\.forest\.car\.zst$").expect("valid regex"));

/// The subset of the snapshot `.metadata.json` file published in the Forest archive
/// that is needed to locate the extended snapshots.
#[derive(Debug, Deserialize)]
struct SnapshotMetadataFile {
    #[serde(rename = "Snapshot")]
    snapshot: SnapshotMetadataSection,
}

#[derive(Debug, Deserialize)]
struct SnapshotMetadataSection {
    #[serde(rename = "Extended", default)]
    extended: Option<ExtendedSnapshots>,
}

/// Auxiliary snapshots that enrich a base snapshot, see `Forest.ChainExport`.
#[derive(Debug, Default, Deserialize, PartialEq)]
struct ExtendedSnapshots {
    /// Message receipts and events
    #[serde(rename = "Augmented", default)]
    receipts_events: Option<ExtendedSnapshotFile>,
    /// Tipset lookup table (checkpoint epoch to tipset key)
    #[serde(rename = "Tipset lookup", default)]
    tipset_lookup: Option<ExtendedSnapshotFile>,
}

#[derive(Debug, Deserialize, PartialEq)]
struct ExtendedSnapshotFile {
    #[serde(rename = "Filename")]
    filename: String,
    #[serde(rename = "Sha256")]
    #[serde(with = "crate::utils::encoding::hex::serde")]
    sha256: [u8; 32],
}

/// Fetches the `.metadata.json` published next to `snapshot_url` and returns its extended
/// snapshots, if any.
async fn fetch_extended_snapshots(snapshot_url: &Url) -> anyhow::Result<Option<ExtendedSnapshots>> {
    let metadata_url = Url::parse(&format!("{snapshot_url}.metadata.json"))?;
    let response = crate::utils::net::global_http_client()
        .get(metadata_url.clone())
        .send()
        .await?;
    let metadata: SnapshotMetadataFile = response
        .error_for_status()?
        .json()
        .await
        .with_context(|| format!("failed to parse snapshot metadata at {metadata_url}"))?;
    Ok(metadata.snapshot.extended)
}

async fn download_extended_snapshot(
    snapshot_url: &Url,
    file: &ExtendedSnapshotFile,
    temp_dir: &Path,
) -> anyhow::Result<tempfile::TempPath> {
    use sha2::{Digest as _, Sha256};

    anyhow::ensure!(
        FILENAME_RE.is_match(&file.filename),
        "invalid extended snapshot filename: {}",
        file.filename
    );
    let url = snapshot_url.join(&file.filename)?;
    let path = new_forest_car_temp_path_in(temp_dir)?;
    info!("Downloading extended snapshot: {url}");
    download_to(&url, &path, DownloadFileOption::Resumable, None).await?;

    let actual: [u8; 32] = Sha256::digest(tokio::fs::read(&path).await?).into();
    if actual != file.sha256 {
        anyhow::bail!("checksum mismatch for {}", file.filename);
    }
    Ok(path)
}

/// Imports message receipts and events from a receipts and events snapshot into `db`.
async fn import_receipts_events_snapshot<DB>(db: &DB, path: &Path) -> anyhow::Result<usize>
where
    DB: Blockstore,
{
    let mut car_stream = CarStream::new_from_path(path).await?;
    let mut n_blocks = 0;
    while let Some(CarBlock { cid, data }) = car_stream.try_next().await? {
        db.put_keyed(&cid, &data)?;
        n_blocks += 1;
    }
    Ok(n_blocks)
}

/// Populates the tipset lookup table in `db` from a tipset lookup snapshot.
async fn import_tipset_lookup_snapshot<DB>(
    db: &Arc<DB>,
    path: &Path,
    head_epoch: ChainEpoch,
) -> anyhow::Result<usize>
where
    DB: EthMappingsStore + Send + Sync + 'static,
{
    let store = MemoryDB::default();
    let mut car_stream = CarStream::new_from_path(path).await?;
    let hamt_root = *car_stream.header_v1.roots.first();
    while let Some(CarBlock { cid, data }) = car_stream.try_next().await? {
        store.put_keyed(&cid, &data)?;
    }

    let db = db.clone();
    tokio::task::spawn_blocking(move || {
        let hamt: Hamt<_, TipsetKey, ChainEpoch> =
            Hamt::load_with_bit_width(&hamt_root, &store, TIPSET_LOOKUP_HAMT_BIT_WIDTH)
                .context("failed to load tipset lookup HAMT")?;
        // Validate all entries before writing any of them.
        let mut n_entries = 0;
        hamt.for_each_cacheless(|&epoch, _| {
            anyhow::ensure!(
                ChainIndex::is_tipset_lookup_checkpoint(epoch) && epoch <= head_epoch,
                "invalid tipset lookup entry at epoch {epoch}, head epoch: {head_epoch}",
            );
            n_entries += 1;
            Ok(())
        })?;
        hamt.for_each_cacheless(|&epoch, tsk| db.set_tipset_key_at_epoch_raw(epoch, tsk))?;
        Ok(n_entries)
    })
    .await?
}

/// Downloads the extended snapshots (message receipts and events, tipset lookup table)
/// advertised in the metadata of the snapshot at `snapshot_path` and imports them into `db`.
/// Does nothing when `snapshot_path` is a local file rather than a URL.
pub async fn maybe_import_extended_snapshots<DB>(
    db: &Arc<DB>,
    snapshot_path: &Path,
    head: &Tipset,
    temp_dir: &Path,
) where
    DB: Blockstore + EthMappingsStore + Send + Sync + 'static,
{
    // Extended snapshots are only published next to remote snapshots.
    let Some(snapshot_url) = path_as_url(snapshot_path) else {
        return;
    };
    let snapshot_url = &snapshot_url;
    let extended = match fetch_extended_snapshots(snapshot_url).await {
        Ok(Some(extended)) => extended,
        Ok(None) => {
            info!("No extended snapshots advertised for {snapshot_url}");
            return;
        }
        Err(e) => {
            warn!("Failed to fetch snapshot metadata for {snapshot_url}: {e:#}");
            return;
        }
    };

    if let Some(file) = &extended.receipts_events {
        let result = async {
            let path = download_extended_snapshot(snapshot_url, file, temp_dir).await?;
            import_receipts_events_snapshot(db, &path).await
        }
        .await;
        match result {
            Ok(n) => info!(
                "Imported {n} message receipt and event blocks from {}",
                file.filename
            ),
            Err(e) => warn!(
                "Failed to import receipts and events snapshot {}: {e:#}",
                file.filename
            ),
        }
    }

    if let Some(file) = &extended.tipset_lookup {
        let result = async {
            let path = download_extended_snapshot(snapshot_url, file, temp_dir).await?;
            import_tipset_lookup_snapshot(db, &path, head.epoch()).await
        }
        .await;
        match result {
            Ok(n) => info!("Imported {n} tipset lookup entries from {}", file.filename),
            Err(e) => warn!(
                "Failed to import tipset lookup snapshot {}: {e:#}",
                file.filename
            ),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::utils::encoding::hex;

    #[test]
    fn parse_snapshot_metadata_with_extended() {
        let json = r#"{
            "Snapshot": {
                "Snapshot version": "2",
                "Epoch": 4137600,
                "Extended": {
                    "Augmented": {
                        "Filename": "forest_snapshot_calibnet_2026-10-08_height_4137600_receipts_events.forest.car.zst",
                        "Sha256": "9585fa94f3c84a117929dd1942014a6f4f79f47dbfdbf693122ff9acfa1311cc"
                    },
                    "Tipset lookup": {
                        "Filename": "forest_snapshot_calibnet_2026-10-08_height_4137600_tipset_lookup.forest.car.zst",
                        "Sha256": "710262581a4fdea6a8cd162e4a65b42727475b83c44b0b0fe05aff72ce70c2f2"
                    }
                }
            },
            "Build Information": {}
        }"#;
        let metadata: SnapshotMetadataFile = serde_json::from_str(json).unwrap();
        let extended = metadata.snapshot.extended.unwrap();
        assert_eq!(
            extended.receipts_events.unwrap().filename,
            "forest_snapshot_calibnet_2026-10-08_height_4137600_receipts_events.forest.car.zst"
        );
        assert_eq!(
            hex::encode(extended.tipset_lookup.unwrap().sha256),
            "710262581a4fdea6a8cd162e4a65b42727475b83c44b0b0fe05aff72ce70c2f2"
        );
    }

    #[test]
    fn parse_snapshot_metadata_without_extended() {
        let json = r#"{ "Snapshot": { "Snapshot version": "2", "Epoch": 4137600 } }"#;
        let metadata: SnapshotMetadataFile = serde_json::from_str(json).unwrap();
        assert!(metadata.snapshot.extended.is_none());
    }
}
