// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use cid::{multihash::Blake2b256, Cid};
use encoding::{from_slice, to_vec};
use forest_ipld::{ipld, Ipld};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone)]
struct TestStruct {
    name: String,
    details: Cid,
}

#[test]
fn encode_new_type() {
    let details = Cid::new_from_cbor(&[1, 2, 3], Blake2b256);
    let name = "Test".to_string();
    let t_struct = TestStruct {
        name: name.clone(),
        details: details.clone(),
    };
    let struct_encoded = to_vec(&t_struct).unwrap();

    // Test to make sure struct can be encoded and decoded without IPLD
    let struct_decoded: TestStruct = from_slice(&struct_encoded).unwrap();
    assert_eq!(&struct_decoded.name, &name);
    assert_eq!(&struct_decoded.details, &details.clone());

    // Test ipld decoding
    let ipld_decoded: Ipld = from_slice(&struct_encoded).unwrap();
    assert_eq!(
        &ipld_decoded,
        &ipld!({"details": Link(details), "name": "Test"})
    );
}
