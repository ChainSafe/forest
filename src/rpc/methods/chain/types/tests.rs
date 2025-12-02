// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;

#[test]
fn test_tipset_selector_success_1() {
    let s = TipsetSelector {
        key: Some(TipsetKey::default()).into(),
        height: None,
        tag: None,
    };
    s.validate().unwrap();
}

#[test]
fn test_tipset_selector_success_2() {
    let s = TipsetSelector {
        key: None.into(),
        height: Some(TipsetHeight {
            at: 100,
            previous: true,
            anchor: None,
        }),
        tag: None,
    };
    s.validate().unwrap();
}

#[test]
fn test_tipset_selector_success_3() {
    let s = TipsetSelector {
        key: None.into(),
        height: None,
        tag: Some(TipsetTag::Finalized),
    };
    s.validate().unwrap();
}

#[test]
fn test_tipset_selector_failure_1() {
    let s = TipsetSelector {
        key: None.into(),
        height: None,
        tag: None,
    };
    s.validate().unwrap_err();
}

#[test]
fn test_tipset_selector_failure_2() {
    let s = TipsetSelector {
        key: Some(TipsetKey::default()).into(),
        height: Some(TipsetHeight {
            at: 100,
            previous: true,
            anchor: None,
        }),
        tag: None,
    };
    s.validate().unwrap_err();
}

#[test]
fn test_tipset_selector_failure_3() {
    let s = TipsetSelector {
        key: Some(TipsetKey::default()).into(),
        height: None,
        tag: Some(TipsetTag::Finalized),
    };
    s.validate().unwrap_err();
}

#[test]
fn test_tipset_selector_failure_4() {
    let s = TipsetSelector {
        key: None.into(),
        height: Some(TipsetHeight {
            at: 100,
            previous: true,
            anchor: None,
        }),
        tag: Some(TipsetTag::Finalized),
    };
    s.validate().unwrap_err();
}

#[test]
fn test_tipset_selector_failure_5() {
    let s = TipsetSelector {
        key: Some(TipsetKey::default()).into(),
        height: Some(TipsetHeight {
            at: 100,
            previous: true,
            anchor: None,
        }),
        tag: Some(TipsetTag::Finalized),
    };
    s.validate().unwrap_err();
}
