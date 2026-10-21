// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Download and import of the extended snapshots (receipts/events and tipset lookup)
//! published next to a snapshot in the Forest archive.

use crate::blocks::{Tipset, TipsetKey};
use crate::chain::{TIPSET_LOOKUP_HAMT_BIT_WIDTH, index::ChainIndex};
use crate::daemon::db_util::{
    BackfillGuard, BackfillOptions, RangeSpec, path_as_url, run_backfill,
};
use crate::db::EthMappingsStore;
use crate::db::car::AnyCar;
use crate::db::car::forest::new_forest_car_temp_path_in;
use crate::prelude::*;
use crate::shim::clock::ChainEpoch;
use crate::state_manager::StateManager;
use crate::utils::db::car_stream::CarStream;
use crate::utils::net::{DownloadFileOption, download_to};
use anyhow::Context;
use fil_actors_shared::fvm_ipld_hamt::Hamt;
use futures::TryStreamExt;
use regex::Regex;
use serde::Deserialize;
use std::path::Path;
use std::time::Duration;
use tracing::{info, warn};
use url::Url;

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
async fn fetch_extended_snapshots_info(
    snapshot_url: &Url,
) -> anyhow::Result<Option<ExtendedSnapshots>> {
    let metadata_url = Url::parse(&format!("{snapshot_url}.metadata.json"))?;
    let response = crate::utils::net::global_http_client()
        .get(metadata_url.clone())
        .timeout(Duration::from_secs(30))
        .send()
        .await?;
    let metadata: SnapshotMetadataFile = response
        .error_for_status()?
        .json()
        .await
        .with_context(|| format!("failed to parse snapshot metadata at {metadata_url}"))?;
    Ok(metadata.snapshot.extended)
}

/// Downloads the extended snapshot `file`, published next to `snapshot_url`, into `temp_dir`
/// and verifies it against the SHA-256 from the snapshot metadata.
async fn download_extended_snapshot(
    snapshot_url: &Url,
    file: &ExtendedSnapshotFile,
    temp_dir: &Path,
) -> anyhow::Result<tempfile::TempPath> {
    use sha2::{Digest as _, Sha256};

    let filename_re = Regex::new(r"^[A-Za-z0-9_.-]+\.forest\.car\.zst$")?;
    anyhow::ensure!(
        filename_re.is_match(&file.filename),
        "invalid extended snapshot filename: {}",
        file.filename
    );
    let url = snapshot_url.join(&file.filename)?;
    let path = new_forest_car_temp_path_in(temp_dir)?;
    info!("Downloading extended snapshot: {url}");
    download_to(&url, &path, DownloadFileOption::Resumable, None).await?;

    let actual: [u8; 32] = Sha256::digest(tokio::fs::read(&path).await?).into();
    anyhow::ensure!(
        actual == file.sha256,
        "checksum mismatch for {}",
        file.filename
    );
    Ok(path)
}

/// Imports message receipts and events from a receipts and events snapshot into `db`.
async fn import_receipts_events_snapshot<DB>(db: &DB, path: &Path) -> anyhow::Result<()>
where
    DB: Blockstore,
{
    let mut car_stream = CarStream::new_from_path(path).await?;
    while let Some(block) = car_stream.try_next().await? {
        block.validate()?;
        db.put_keyed(&block.cid, &block.data)?;
    }
    Ok(())
}

/// Populates the tipset lookup table in `db` from a tipset lookup snapshot, calling
/// `on_imported` after each entry is written. Every entry is validated against the tipsets
/// in `db` before any of them is written.
pub fn import_tipset_lookup_snapshot<DB>(
    db: &DB,
    path: &Path,
    mut on_imported: impl FnMut(),
) -> anyhow::Result<()>
where
    DB: Blockstore + EthMappingsStore,
{
    let hamt_car = AnyCar::try_from(path)?;
    anyhow::ensure!(
        hamt_car.header_v1().roots.len() == 1,
        "hamt car should have a single root"
    );
    let hamt_root = *hamt_car.header_v1().roots.first();
    let hamt: Hamt<_, TipsetKey, ChainEpoch> =
        Hamt::load_with_bit_width(&hamt_root, hamt_car, TIPSET_LOOKUP_HAMT_BIT_WIDTH)
            .context("failed to load tipset lookup HAMT")?;
    hamt.for_each_cacheless(|&epoch, tsk| {
        anyhow::ensure!(
            ChainIndex::is_tipset_lookup_checkpoint(epoch),
            "epoch {epoch} is not a tipset lookup checkpoint"
        );
        let ts = Tipset::load_required(db, tsk)?;
        anyhow::ensure!(
            ts.epoch() == epoch,
            "epochs do not match, {epoch} in hamt, {} in database",
            ts.epoch()
        );
        Ok(())
    })?;
    hamt.for_each_cacheless(|&epoch, tsk| {
        db.set_tipset_key_at_epoch_raw(epoch, tsk)?;
        on_imported();
        Ok(())
    })?;
    Ok(())
}

/// Indexes the tipsets whose receipts and events were just imported (block logs blooms and
/// Ethereum mappings), as `forest-tool index backfill` does.
async fn backfill_receipts_events(
    state_manager: &StateManager,
    head: &Tipset,
) -> anyhow::Result<()> {
    let db = state_manager.db();
    let from = Tipset::load_required(db, head.parents())?;
    let Some(to) = from
        .shallow_clone()
        .chain(db)
        .take_while(|ts| db.has(ts.parent_state()).unwrap_or_default())
        .last()
        .map(|ts| ts.epoch())
    else {
        return Ok(());
    };
    let guard = BackfillGuard::try_start()?;
    let options = BackfillOptions {
        allow_recompute: false,
        ..Default::default()
    };
    let result = run_backfill(state_manager, &from, RangeSpec::To(to), options, &guard).await;
    let report = guard.finish(result)?;
    if report.skipped > 0 {
        warn!(
            "Indexed {} tipsets from epoch {} to {to}, {} could not be indexed",
            report.indexed,
            from.epoch(),
            report.skipped
        );
    } else {
        info!(
            "Indexed {} tipsets from epoch {} to {to}",
            report.indexed,
            from.epoch()
        );
    }
    Ok(())
}

/// Downloads the extended snapshots (message receipts and events, tipset lookup table)
/// advertised in the metadata of the snapshot at `snapshot_path` and imports them into the
/// database of `state_manager`. Does nothing when `snapshot_path` is a local file rather than a URL.
pub(super) async fn maybe_import_extended_snapshots(
    state_manager: &StateManager,
    head: &Tipset,
    snapshot_path: &Path,
    temp_dir: &Path,
) {
    // Extended snapshots are only published next to remote snapshots.
    let Some(snapshot_url) = path_as_url(snapshot_path) else {
        return;
    };
    let extended = match fetch_extended_snapshots_info(&snapshot_url).await {
        Ok(Some(extended)) => extended,
        Ok(None) => {
            info!("No extended snapshots advertised for {snapshot_url}");
            return;
        }
        Err(e) => {
            warn!("Failed to fetch extended snapshots for {snapshot_url}: {e:#}");
            return;
        }
    };

    let receipts_events_imported = match &extended.receipts_events {
        Some(file) => {
            let result = async {
                let path = download_extended_snapshot(&snapshot_url, file, temp_dir).await?;
                import_receipts_events_snapshot(state_manager.db(), &path).await
            }
            .await;
            match result {
                Ok(()) => {
                    info!(
                        "Imported message receipts and events from {}",
                        file.filename
                    );
                    true
                }
                Err(e) => {
                    warn!(
                        "Failed to import receipts and events snapshot {}: {e:#}",
                        file.filename
                    );
                    false
                }
            }
        }
        None => false,
    };

    if let Some(file) = &extended.tipset_lookup {
        let result = async {
            let path = download_extended_snapshot(&snapshot_url, file, temp_dir).await?;
            let db = state_manager.db_owned();
            tokio::task::spawn_blocking(move || import_tipset_lookup_snapshot(&db, &path, || {}))
                .await?
        }
        .await;
        match result {
            Ok(()) => info!("Imported tipset lookup entries from {}", file.filename),
            Err(e) => warn!(
                "Failed to import tipset lookup snapshot {}: {e:#}",
                file.filename
            ),
        }
    }

    // Index once both snapshots are imported, so each import's logs stay together.
    if receipts_events_imported && let Err(e) = backfill_receipts_events(state_manager, head).await
    {
        warn!("Failed to index imported receipts and events: {e:#}");
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
