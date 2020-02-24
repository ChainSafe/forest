// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#![cfg(feature = "serde_derive")]

use forest_cid::Cid;
use multihash;
use multihash::Hash::Blake2b256;
use serde_cbor::{from_slice, to_vec};

#[test]
fn vector_cid_serialize_round() {
    let cids = vec![
        Cid::from_bytes(&[0, 1], Blake2b256).unwrap(),
        Cid::from_bytes(&[1, 2], Blake2b256).unwrap(),
        Cid::from_bytes(&[3, 2], Blake2b256).unwrap(),
    ];

    // Serialize cids with cbor
    let enc = to_vec(&cids).unwrap();

    // decode cbor bytes to vector again
    let dec: Vec<Cid> = from_slice(&enc).unwrap();

    assert_eq!(cids, dec);
}
