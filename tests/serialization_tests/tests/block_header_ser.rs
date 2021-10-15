// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#![cfg(feature = "submodule_tests")]

use cid::Cid;
use encoding::to_vec;
use forest_blocks::{header, BlockHeader};
use hex::encode;
use serde::Deserialize;
use std::fs::File;
use std::io::prelude::*;

#[derive(Deserialize)]
struct BlockHeaderVector {
    #[serde(with = "header::json")]
    block: BlockHeader,
    cbor_hex: String,
    cid: String,
}

fn encode_assert_cbor(header: &BlockHeader, expected: &str, cid: &Cid) {
    let enc_bz: Vec<u8> = to_vec(header).expect("Cbor serialization failed");

    // Assert the header is encoded in same format
    assert_eq!(encode(enc_bz.as_slice()), expected);
    // Assert decoding from those bytes goes back to unsigned header
    assert_eq!(header.cid(), cid);
}

#[test]
fn header_cbor_vectors() {
    let mut file = File::open("serialization-vectors/block_headers.json").unwrap();
    let mut string = String::new();
    file.read_to_string(&mut string).unwrap();

    let vectors: Vec<BlockHeaderVector> =
        serde_json::from_str(&string).expect("Test vector deserialization failed");
    for tv in vectors {
        encode_assert_cbor(
            &BlockHeader::from(tv.block),
            &tv.cbor_hex,
            &tv.cid.parse().unwrap(),
        )
    }
}
