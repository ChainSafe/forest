// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use cid::Cid;
use forest_blocks::{header, BlockHeader};
use forest_shim::address::{set_current_network, Network};
use fvm_ipld_encoding::to_vec;
use hex::encode;
use serde::Deserialize;

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
    set_current_network(Network::Testnet);

    let s = include_str!("../../../serialization-vectors/block_headers.json");

    let vectors: Vec<BlockHeaderVector> =
        serde_json::from_str(s).expect("Test vector deserialization failed");

    for tv in vectors {
        encode_assert_cbor(&tv.block, &tv.cbor_hex, &tv.cid.parse().unwrap())
    }
}
