// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#![cfg(feature = "cbor")]

use forest_cid::Cid;
use multihash::Blake2b256;
use serde_cbor::{from_slice, to_vec};

#[test]
fn vector_cid_serialize_round() {
    let cids = vec![
        Cid::new_from_cbor(&[0, 1], Blake2b256),
        Cid::new_from_cbor(&[1, 2], Blake2b256),
        Cid::new_from_cbor(&[3, 2], Blake2b256),
    ];

    // Serialize cids with cbor
    let enc = to_vec(&cids).unwrap();

    // decode cbor bytes to vector again
    let dec: Vec<Cid> = from_slice(&enc).unwrap();

    assert_eq!(cids, dec);
}
