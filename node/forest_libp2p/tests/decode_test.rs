// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#![cfg(test)]

use forest_blocks::FullTipset;
use forest_libp2p::rpc::{BlockSyncResponse, TipSetBundle};

#[test]
fn convert_single_tipset_bundle() {
    let bundle = TipSetBundle {
        blocks: Vec::new(),
        bls_msgs: Vec::new(),
        bls_msg_includes: Vec::new(),
        secp_msgs: Vec::new(),
        secp_msg_includes: Vec::new(),
    };

    let res = BlockSyncResponse {
        chain: vec![bundle],
        status: 0,
        message: "".into(),
    }
    .into_result()
    .unwrap();

    assert_eq!(res, [FullTipset::new(vec![])]);
}

#[test]
fn convert_example_tipset_bundle() {
    // TODO
}

#[test]
fn convert_actual_tipset_bundle() {
    // TODO
}
