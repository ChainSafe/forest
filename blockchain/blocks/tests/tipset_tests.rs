// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use forest_blocks::*;
use num_bigint::BigUint;
use test_utils::{construct_header, construct_tipset, key_setup, template_key};

const WEIGHT: u64 = 10;

#[test]
fn new_test() {
    let headers = construct_header(0);
    assert!(Tipset::new(headers).is_ok(), "result is invalid");
}

#[test]
fn min_ticket_test() {
    let tipset = construct_tipset(0);
    let expected_value: &[u8] = &[1, 4, 3, 6, 1, 1, 2, 2, 4, 5, 3, 12, 2];
    let min = Tipset::min_ticket(&tipset).unwrap();
    assert_eq!(min.vrfproof.bytes(), expected_value);
}

#[test]
fn min_timestamp_test() {
    let tipset = construct_tipset(0);
    let min_time = Tipset::min_timestamp(&tipset).unwrap();
    assert_eq!(min_time, 1);
}

#[test]
fn len_test() {
    let tipset = construct_tipset(0);
    assert_eq!(Tipset::len(&tipset), 3);
}

#[test]
fn is_empty_test() {
    let tipset = construct_tipset(0);
    assert_eq!(Tipset::is_empty(&tipset), false);
}

#[test]
fn parents_test() {
    let tipset = construct_tipset(0);
    let expected_value = template_key(b"test content");
    assert_eq!(
        *tipset.parents(),
        TipSetKeys {
            cids: vec!(expected_value)
        }
    );
}

#[test]
fn weight_test() {
    let tipset = construct_tipset(0);
    assert_eq!(tipset.weight(), &BigUint::from(WEIGHT));
}

#[test]
fn equals_test() {
    let tipset_keys = TipSetKeys {
        cids: key_setup().clone(),
    };
    let tipset_keys2 = TipSetKeys {
        cids: key_setup().clone(),
    };
    assert_eq!(tipset_keys.equals(&tipset_keys2), true);
}
