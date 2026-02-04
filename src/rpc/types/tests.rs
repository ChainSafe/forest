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
