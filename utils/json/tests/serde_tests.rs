// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#![cfg(feature = "cbor")]

use cid::{
    multihash::{Code::Blake2b256, MultihashDigest},
    Cid,
};
use fvm_ipld_encoding::DAG_CBOR;
use serde_ipld_dagcbor::{from_slice, to_vec};

#[test]
fn vector_cid_serialize_round() {
    let cids = vec![
        Cid::new_v1(DAG_CBOR, Blake2b256.digest(&[0, 1])),
        Cid::new_v1(DAG_CBOR, Blake2b256.digest(&[1, 2])),
        Cid::new_v1(DAG_CBOR, Blake2b256.digest(&[3, 2])),
    ];

    // Serialize cids with cbor
    let enc = to_vec(&cids).unwrap();

    // decode cbor bytes to vector again
    let dec: Vec<Cid> = from_slice(&enc).unwrap();

    assert_eq!(cids, dec);
}
