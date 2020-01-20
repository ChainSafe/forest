// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

use cid::Cid;
use encoding::{from_slice, to_vec};
use ferret_ipld::Ipld;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Serialize, Deserialize, Clone)]
struct TestStruct {
    name: String,
    details: Cid,
}

#[test]
fn encode_new_type() {
    let details = Cid::from_bytes_default(&[1, 2, 3]).unwrap();
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
    let mut e_map = BTreeMap::<String, Ipld>::new();
    e_map.insert("details".to_string(), Ipld::Link(details));
    e_map.insert("name".to_string(), Ipld::String(name));
    assert_eq!(&ipld_decoded, &Ipld::Map(e_map));
}
