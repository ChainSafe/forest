// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;

#[test]
fn test_tipset_selector_serde() {
    let s = TipsetSelector {
        key: None.into(),
        height: None,
        tag: None,
    };
    let json = serde_json::to_value(&s).unwrap();
    println!("{json}");
    let s2: TipsetSelector = serde_json::from_value(json).unwrap();
    assert_eq!(s, s2);
}
