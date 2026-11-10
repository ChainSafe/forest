// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Download and import of the extended snapshots (receipts/events and tipset lookup)
//! published next to a snapshot in the Forest archive.

use crate::blocks::Tipset;
use crate::chain::{ChainEpochDelta, import_tipset_lookup_snapshot};
use crate::daemon::db_util::{
    BackfillGuard, BackfillOptions, BackfillReport, RangeSpec, run_backfill,
};
use crate::db::car::forest::new_forest_car_temp_path_in;
use crate::prelude::*;
use crate::state_manager::StateManager;
use crate::utils::db::car_util::import_car_blocks;
use crate::utils::io::progress_log::Progress;
use crate::utils::net::{DownloadFileOption, download_to_with_timeout};
use anyhow::Context;
use regex::Regex;
use serde::Deserialize;
use sha2::Sha256;
use std::path::Path;
use std::time::{Duration, Instant};
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
    /// Number of epochs below the head with state, which the receipts and events also cover
    #[serde(rename = "State-roots")]
    state_roots: ChainEpochDelta,
    #[serde(rename = "Extended")]
    extended: Option<ExtendedSnapshots>,
}

/// Auxiliary snapshots that enrich a base snapshot, see `Forest.ChainExport`.
#[derive(Debug, Deserialize)]
struct ExtendedSnapshots {
    /// Message receipts and events
    #[serde(rename = "Augmented")]
    receipts_events: Option<ExtendedSnapshotFile>,
    /// Tipset lookup table (checkpoint epoch to tipset key)
    #[serde(rename = "Tipset lookup")]
    tipset_lookup: Option<ExtendedSnapshotFile>,
}

#[derive(Debug, Deserialize)]
struct ExtendedSnapshotFile {
    #[serde(rename = "Filename")]
    filename: String,
    #[serde(rename = "Sha256")]
    #[serde(with = "crate::utils::encoding::hex::serde")]
    sha256: [u8; 32],
}

/// Fetches the `.metadata.json` published next to `snapshot_url`.
async fn fetch_metadata_snapshots_info(
    snapshot_url: &Url,
) -> anyhow::Result<SnapshotMetadataSection> {
    // Append to the path, not the whole URL, so dont affect any query string.
    let mut metadata_url = snapshot_url.clone();
    metadata_url.set_path(&format!("{}.metadata.json", snapshot_url.path()));
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
    Ok(metadata.snapshot)
}

/// Downloads the extended snapshot `file`, published next to `snapshot_url`, into `temp_dir`
/// and verifies it against the SHA-256 from the snapshot metadata.
async fn download_extended_snapshot(
    snapshot_url: &Url,
    file: &ExtendedSnapshotFile,
    temp_dir: &Path,
) -> anyhow::Result<tempfile::TempPath> {
    let filename_re = Regex::new(r"^[A-Za-z0-9_.-]+\.forest\.car\.zst$")?;
    anyhow::ensure!(
        filename_re.is_match(&file.filename),
        "invalid extended snapshot filename: {}",
        file.filename
    );
    let mut url = snapshot_url.join(&file.filename)?;
    url.set_query(snapshot_url.query());

    let path = new_forest_car_temp_path_in(temp_dir)?;
    info!("Downloading extended snapshot: {url}");
    download_to_with_timeout(
        &url,
        &path,
        DownloadFileOption::Resumable,
        Duration::from_mins(10),
        None,
    )
    .await
    .with_context(|| format!("failed to download {}", file.filename))?;

    let path_for_hash = path.to_path_buf();
    let actual: [u8; 32] = tokio::task::spawn_blocking(move || {
        crate::utils::hash::digest_file::<Sha256>(&path_for_hash)
    })
    .await
    .with_context(|| format!("failed to hash {}", file.filename))?
    .with_context(|| format!("failed to read {} for hashing", file.filename))?
    .into();
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
    let started = Instant::now();
    import_car_blocks(db, path, true, || {}).await?;
    info!(
        "Imported message receipts and events in {:.1}s",
        started.elapsed().as_secs_f64()
    );
    Ok(())
}

/// Indexes the tipsets whose receipts and events were just imported (block logs blooms and
/// Ethereum mappings), as `forest-tool index backfill` does.
async fn backfill_receipts_events(
    state_manager: &StateManager,
    head: &Tipset,
    depth: ChainEpochDelta,
) -> anyhow::Result<BackfillReport> {
    let from = Tipset::load_required(state_manager.db(), head.parents())?;
    // the lowest tipset that can be indexed is at `head - depth + 1`
    let to = head.epoch() - depth.clamp(0, head.epoch()) + 1;
    let started = Instant::now();
    let guard = BackfillGuard::try_start()?;
    let options = BackfillOptions {
        allow_recompute: false,
        ..Default::default()
    };
    let result = run_backfill(state_manager, &from, RangeSpec::To(to), options, &guard).await;
    let report = guard.finish(result)?;

    // index ethereum events from the head
    let chain_store = state_manager.chain_store();
    let head_delegated_messages =
        chain_store.headers_delegated_messages(head.block_headers().iter())?;
    chain_store.process_signed_messages(&head_delegated_messages, true)?;
    if report.skipped > 0 {
        warn!(
            "Indexed {} tipsets from epoch {} to {to}, {} could not be indexed",
            report.indexed,
            from.epoch(),
            report.skipped
        );
    } else {
        info!(
            "Indexed {} tipsets from epoch {} to {to} in {:.1}s",
            report.indexed,
            from.epoch(),
            started.elapsed().as_secs_f64()
        );
    }
    Ok(report)
}

/// Downloads the extended snapshots (message receipts and events, tipset lookup table)
/// advertised in the metadata of the snapshot at `snapshot_url` and imports them into the
/// database of `state_manager`.
pub(super) async fn maybe_import_extended_snapshots(
    state_manager: &StateManager,
    head: &Tipset,
    snapshot_url: &Url,
    temp_dir: &Path,
) {
    let metadata = match fetch_metadata_snapshots_info(snapshot_url).await {
        Ok(metadata) => metadata,
        Err(e) => {
            warn!(
                "Failed to fetch extended snapshots for {snapshot_url}: {e:#}; continuing without \
                 receipts and events, import them manually, see the enriched snapshots docs"
            );
            return;
        }
    };
    let Some(extended) = metadata.extended else {
        info!("No extended snapshots advertised for {snapshot_url}");
        return;
    };

    let receipts_events_imported = match &extended.receipts_events {
        Some(file) => {
            let result = async {
                let path = download_extended_snapshot(snapshot_url, file, temp_dir).await?;
                import_receipts_events_snapshot(state_manager.db(), &path).await
            }
            .await;
            if let Err(e) = &result {
                warn!(
                    "Failed to import receipts and events snapshot {}: {e:#}; continuing without \
                     them, import them manually, see the enriched snapshots docs",
                    file.filename
                );
            }
            result.is_ok()
        }
        None => false,
    };

    if let Some(file) = &extended.tipset_lookup {
        let result = async {
            let path = download_extended_snapshot(snapshot_url, file, temp_dir).await?;
            let db = state_manager.db_owned();
            tokio::task::spawn_blocking(move || {
                let mut progress = Progress::new("Importing tipset lookup entries");
                import_tipset_lookup_snapshot(&db, &path, || progress.inc(1))
            })
            .await?
        }
        .await;
        if let Err(e) = result {
            warn!(
                "Failed to import tipset lookup snapshot {}: {e:#}; continuing without the tipset \
                 lookup table, import it manually, see the enriched snapshots docs",
                file.filename
            );
        }
    }

    // Index once both snapshots are imported, so each import's logs stay together.
    if receipts_events_imported
        && let Err(e) = backfill_receipts_events(state_manager, head, metadata.state_roots).await
    {
        warn!(
            "Failed to index imported receipts and events: {e:#}; continuing without the bloom \
             index and Ethereum mappings, run forest-tool index backfill, see the enriched \
             snapshots docs"
        );
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::blocks::{CachingBlockHeader, RawBlockHeader};
    use crate::chain::index::tests::{genesis_tipset, persist_tipset, tipset_child};
    use crate::chain_sync::TipsetValidator;
    use crate::rpc::eth::eth_tx_from_signed_eth_message;
    use crate::test_utils::{construct_eth_messages, dummy_ticket};
    use crate::utils::db::CborStoreExt as _;
    use crate::utils::db::car_stream::CarBlock;
    use crate::utils::encoding::hex;
    use crate::utils::multihash::prelude::*;
    use fvm_ipld_encoding::DAG_CBOR;
    use nunny::vec as nonempty;
    use rstest::rstest;

    // The default fixture: a head with a single parent.
    fn state_manager_with_delegated_head() -> (
        StateManager,
        Tipset,
        crate::message::SignedMessage,
        crate::rpc::eth::types::EthHash,
    ) {
        state_manager_with_delegated_head_chain(1)
    }

    // Builds a chain of `parents_below` tipsets ending in a head that carries a single delegated
    // message, and returns the state manager, the head, the message and the message's Ethereum
    // hash.
    fn state_manager_with_delegated_head_chain(
        parents_below: ChainEpoch,
    ) -> (
        StateManager,
        Tipset,
        crate::message::SignedMessage,
        crate::rpc::eth::types::EthHash,
    ) {
        let sm = crate::daemon::db_util::test_state_manager();
        let db = sm.db();

        // A delegated message carried by the head block.
        let (_, smsg) = construct_eth_messages(0);
        db.put_cbor_default(&smsg).unwrap();

        // The fixture uses mainnet, which forks to Hygge at 2_683_348; stay above it so
        // `headers_delegated_messages` does not filter the head out.
        let head_epoch = 2_700_000;
        let mut parent = tipset_child(&genesis_tipset(), head_epoch - parents_below);
        persist_tipset(&parent, db);
        for epoch in (head_epoch - parents_below + 1)..head_epoch {
            parent = tipset_child(&parent, epoch);
            persist_tipset(&parent, db);
        }
        let head = Tipset::from(CachingBlockHeader::new(RawBlockHeader {
            parents: parent.key().clone(),
            epoch: head_epoch,
            ticket: dummy_ticket(1),
            messages: TipsetValidator::compute_msg_root(
                db,
                std::iter::empty(),
                [smsg.signed_cid()],
            )
            .unwrap(),
            ..Default::default()
        }));
        persist_tipset(&head, db);

        let (_, tx) =
            eth_tx_from_signed_eth_message(&smsg, sm.chain_store().chain_config().eth_chain_id)
                .unwrap();
        let hash = tx.eth_hash().unwrap().into();
        (sm, head, smsg, hash)
    }

    // The walk covers the head's parent down to `head - depth + 1`, both inclusive. The fixture
    // tipsets carry no executed state, so each one is skipped and the counters give the range.
    // The head is the one tipset the walk cannot reach, so its delegated message is indexed
    // separately.
    #[tokio::test]
    #[serial_test::serial(chain_export)]
    async fn backfill_receipts_events_indexes_range_and_head() {
        let (sm, head, smsg, hash) = state_manager_with_delegated_head_chain(3);

        let report = backfill_receipts_events(&sm, &head, 3).await.unwrap();

        assert_eq!(report.skipped, 2);
        assert_eq!(report.indexed, 0);
        assert_eq!(
            sm.chain_store().get_mapping(&hash).unwrap(),
            Some(smsg.cid())
        );
    }

    // A receipts and events CAR with one valid block. The orchestration tests only need the import
    // to succeed; the fixture's tipsets carry no executed state, so the walk indexes no tipset
    // keys and the head mapping is the observable effect of the backfill.
    async fn receipts_events_car() -> Vec<u8> {
        let data = b"receipts and events".to_vec();
        let cid = Cid::new_v1(DAG_CBOR, MultihashCode::Blake2b256.digest(&data));
        let block = CarBlock {
            cid,
            data: data.into(),
        };
        let frames =
            crate::db::car::forest::Encoder::compress_stream_default(futures::stream::iter([
                Ok::<_, anyhow::Error>(block),
            ]));
        let mut writer = vec![];
        crate::db::car::forest::Encoder::write(&mut writer, nonempty![cid], frames)
            .await
            .unwrap();
        writer
    }

    fn extended_metadata_json(state_roots: ChainEpoch, sha256_hex: &str) -> String {
        format!(
            r#"{{ 
                "Snapshot": {{ 
                    "State-roots": {state_roots}, 
                    "Extended": {{ 
                        "Augmented": {{ 
                            "Filename": 
                            "receipts.forest.car.zst", 
                            "Sha256": "{sha256_hex}" 
                        }} 
                    }} 
                }} 
            }}"#
        )
    }

    // Serves `<base>.metadata.json` and the receipts companion next to `snapshot_url`. Both
    // handlers require the `token=x` query, so a dropped query string fails the import.
    async fn serve_extended_snapshot(metadata: String, car: Vec<u8>) -> Url {
        use axum::response::IntoResponse as _;

        let app = axum::Router::new()
            .route(
                "/base.forest.car.zst.metadata.json",
                axum::routing::get(move |uri: axum::http::Uri| {
                    let metadata = metadata.clone();
                    async move {
                        if uri.query() != Some("token=x") {
                            return (axum::http::StatusCode::FORBIDDEN, String::new())
                                .into_response();
                        }
                        (axum::http::StatusCode::OK, metadata).into_response()
                    }
                }),
            )
            .route(
                "/receipts.forest.car.zst",
                axum::routing::get(move |uri: axum::http::Uri| {
                    let car = car.clone();
                    async move {
                        if uri.query() != Some("token=x") {
                            return (axum::http::StatusCode::FORBIDDEN, Vec::new()).into_response();
                        }
                        (axum::http::StatusCode::OK, car).into_response()
                    }
                }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await });
        Url::parse(&format!("http://{addr}/base.forest.car.zst?token=x")).unwrap()
    }

    // The whole orchestration: metadata lookup and companion download keep the snapshot's query
    // string, the receipts import gates the backfill, and the head tipset's delegated message
    // ends up indexed.
    #[tokio::test]
    #[serial_test::serial(chain_export)]
    async fn maybe_import_extended_snapshots_indexes_head() {
        use sha2::{Digest as _, Sha256};

        let (sm, head, smsg, hash) = state_manager_with_delegated_head();
        let car = receipts_events_car().await;
        let metadata = extended_metadata_json(1, &hex::encode(Sha256::digest(&car)));
        let snapshot_url = serve_extended_snapshot(metadata, car).await;

        let temp_dir = tempfile::tempdir().unwrap();
        maybe_import_extended_snapshots(&sm, &head, &snapshot_url, temp_dir.path()).await;

        assert_eq!(
            sm.chain_store().get_mapping(&hash).unwrap(),
            Some(smsg.cid())
        );
    }

    // A failed receipts import must skip the backfill, so the head's mapping is not written.
    #[tokio::test]
    #[serial_test::serial(chain_export)]
    async fn maybe_import_extended_snapshots_skips_backfill_without_receipts() {
        let (sm, head, _, hash) = state_manager_with_delegated_head();
        let car = receipts_events_car().await;
        // A wrong checksum makes the companion import fail.
        let metadata = extended_metadata_json(1, &hex::encode([0u8; 32]));
        let snapshot_url = serve_extended_snapshot(metadata, car).await;

        let temp_dir = tempfile::tempdir().unwrap();
        maybe_import_extended_snapshots(&sm, &head, &snapshot_url, temp_dir.path()).await;

        assert_eq!(sm.chain_store().get_mapping(&hash).unwrap(), None);
    }

    #[rstest]
    #[case::ok("companion.forest.car.zst", true, true)]
    #[case::checksum_mismatch("companion.forest.car.zst", false, false)]
    #[case::path_traversal("../companion.forest.car.zst", true, false)]
    #[tokio::test]
    async fn download_extended_snapshot_verifies(
        #[case] filename: &str,
        #[case] matching_checksum: bool,
        #[case] succeeds: bool,
    ) {
        use sha2::{Digest as _, Sha256};
        let dummy_content: &'static [u8] = b"companion snapshot bytes";

        let app = axum::Router::new().route(
            "/companion.forest.car.zst",
            axum::routing::get(move || async move { dummy_content }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await });

        let snapshot_url = Url::parse(&format!("http://{addr}/base.forest.car.zst")).unwrap();
        let file = ExtendedSnapshotFile {
            filename: filename.into(),
            sha256: if matching_checksum {
                Sha256::digest(dummy_content).into()
            } else {
                [0; 32]
            },
        };
        let temp_dir = tempfile::tempdir().unwrap();
        let result = download_extended_snapshot(&snapshot_url, &file, temp_dir.path()).await;
        assert_eq!(result.is_ok(), succeeds, "{result:?}");
        if let Ok(path) = result {
            assert_eq!(std::fs::read(path).unwrap(), dummy_content);
        }
    }

    // A stalled companion download must surface as a timeout instead of blocking startup.
    #[tokio::test(start_paused = true)]
    async fn download_extended_snapshot_times_out() {
        let app = axum::Router::new().route(
            "/companion.forest.car.zst",
            axum::routing::get(|| async { std::future::pending::<()>().await }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await });

        let snapshot_url = Url::parse(&format!("http://{addr}/base.forest.car.zst")).unwrap();
        let file = ExtendedSnapshotFile {
            filename: "companion.forest.car.zst".into(),
            sha256: [0; 32],
        };
        let temp_dir = tempfile::tempdir().unwrap();
        let err = download_extended_snapshot(&snapshot_url, &file, temp_dir.path())
            .await
            .unwrap_err();
        assert_eq!(
            err.downcast_ref(),
            Some(&crate::utils::RetryError::TimeoutExceeded)
        );
    }

    #[test]
    fn parse_snapshot_metadata_with_extended() {
        let json = r#"{
            "Snapshot": {
                "Snapshot version": "2",
                "Epoch": 4137600,
                "State-roots": 2000,
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
        assert_eq!(metadata.snapshot.state_roots, 2000);
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
        let json =
            r#"{ "Snapshot": { "Snapshot version": "2", "Epoch": 4137600, "State-roots": 2000 } }"#;
        let metadata: SnapshotMetadataFile = serde_json::from_str(json).unwrap();
        assert!(metadata.snapshot.extended.is_none());
    }

    // Metadata that cannot be fetched is an error whatever the status, a 404 included.
    #[tokio::test]
    async fn fetch_metadata_snapshots_info_missing_is_error() {
        let app = axum::Router::new()
            .route(
                "/base.forest.car.zst.metadata.json",
                axum::routing::get(|| async { r#"{ "Snapshot": { "State-roots": 2000 } }"# }),
            )
            .route(
                "/missing.forest.car.zst.metadata.json",
                axum::routing::get(|| async { axum::http::StatusCode::NOT_FOUND }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await });

        let present = Url::parse(&format!("http://{addr}/base.forest.car.zst")).unwrap();
        let metadata = fetch_metadata_snapshots_info(&present).await.unwrap();
        assert_eq!(metadata.state_roots, 2000);

        let missing = Url::parse(&format!("http://{addr}/missing.forest.car.zst")).unwrap();
        assert!(fetch_metadata_snapshots_info(&missing).await.is_err());
    }
}
