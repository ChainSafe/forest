// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::lotus_json::HasLotusJson as _;
use itertools::Itertools as _;
use quickcheck_macros::quickcheck;

#[quickcheck]
fn test_api_tipset_key(cids: Vec<Cid>) {
    test_api_tipset_key_inner(cids)
}

#[test]
fn test_api_tipset_key_empty() {
    test_api_tipset_key_inner(vec![])
}

#[test]
fn test_api_tipset_key_deserialization_empty_vec() {
    let api_ts_lotus_json: LotusJson<ApiTipsetKey> = serde_json::from_str("[]").unwrap();
    assert!(api_ts_lotus_json.into_inner().0.is_none());
}

#[test]
fn test_api_tipset_key_deserialization_null() {
    let api_ts_lotus_json: LotusJson<ApiTipsetKey> = serde_json::from_str("null").unwrap();
    assert!(api_ts_lotus_json.into_inner().0.is_none());
}

fn test_api_tipset_key_inner(cids: Vec<Cid>) {
    let lotus_json_str = cids.clone().into_lotus_json_string_pretty().unwrap();
    let api_ts_lotus_json: LotusJson<ApiTipsetKey> = serde_json::from_str(&lotus_json_str).unwrap();
    let api_ts = api_ts_lotus_json.into_inner();
    let cids_from_api_ts = api_ts
        .0
        .map(|ts| ts.into_cids().into_iter().collect_vec())
        .unwrap_or_default();
    assert_eq!(cids_from_api_ts, cids);
}

/// Pins the `export-status` text format: a running export always names its kind and,
/// once the walk reports progress, the raw epoch counters.
#[test]
fn api_export_status_display() {
    use crate::ipld::{ChainExportKind, ChainExportState};
    let mut status = ApiExportStatus {
        state: ChainExportState::Running,
        kind: Some(ChainExportKind::Snapshot),
        error: None,
        progress: 0.0,
        start_epoch: 3898735,
        current_epoch: 3898000,
        start_time: None,
    };
    assert_eq!(
        status.to_string(),
        "Exporting (Snapshot): 0.0% (walk at epoch 3898000, counting down from 3898735; started at unknown)"
    );

    status.state = ChainExportState::Failed;
    status.error = Some("missing state root".into());
    assert_eq!(
        status.to_string(),
        "No export in progress (last Snapshot export failed: missing state root)"
    );
}
