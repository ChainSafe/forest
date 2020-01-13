// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

use address::Address;
use encoding::Cbor;
use vm::{MethodNum, MethodParams, Serialized};

#[test]
fn params_usage() {
    // Test to make sure a vector of bytes can be added and removed from params
    let mut params = MethodParams::default();
    params.push(Serialized::new(vec![1, 2]));
    assert_eq!(params.pop().unwrap().bytes(), vec![1, 2]);
}

#[test]
fn cbor_params() {
    // Test cbor encodable objects can be added and removed from parameters
    let mut params = MethodParams::default();
    let addr = Address::new_id(1).unwrap();
    params.insert(0, Serialized::serialize(addr.clone()).unwrap());
    let encoded_addr = params.remove(0);
    assert_eq!(Address::unmarshal_cbor(&encoded_addr).unwrap(), addr);
}

#[test]
fn method_num() {
    // Test constructor available publicly
    let method = MethodNum::new(1);
    assert_eq!(1, method.into());
}
