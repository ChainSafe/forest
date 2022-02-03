// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#![cfg(feature = "cbor")]

use forest_cid::{Cid, Code::Blake2b256};
use serde_cbor::{from_slice, to_vec, Result};

#[test]
fn vector_cid_serialize_round() {
    let cids = vec![
        forest_cid::new_from_cbor(&[0, 1], Blake2b256),
        forest_cid::new_from_cbor(&[1, 2], Blake2b256),
        forest_cid::new_from_cbor(&[3, 2], Blake2b256),
    ];

    // Serialize cids with cbor
    let enc = to_vec(&cids).unwrap();

    // decode cbor bytes to vector again
    let dec: Vec<Cid> = from_slice(&enc).unwrap();

    assert_eq!(cids, dec);
}

const CID_VALUE_START_LOC: usize = 4;

#[test]
fn deserialize_invalid_cid() {
    let cid = forest_cid::new_from_cbor(&[0, 1], Blake2b256);
    let mut enc = to_vec(&cid).unwrap();

    // now modify the bytes for Cid.value
    enc[CID_VALUE_START_LOC] = 1;

    // decode cbor bytes to vector again
    let r: Result<Cid> = from_slice(&enc);
    assert_eq!(r.is_err(), true);
}
