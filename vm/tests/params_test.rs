// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use address::Address;
use encoding::{from_slice, to_vec};
use forest_vm::Serialized;

#[test]
fn serialized_deserialize() {
    // Bytes taken from filecoin network encoded parameters from a message encoding
    let valid_bytes: &[u8] = &[
        88, 122, 129, 129, 137, 88, 32, 253, 47, 195, 200, 241, 49, 105, 17, 23, 102, 198, 44, 98,
        146, 98, 117, 43, 43, 228, 104, 245, 49, 207, 200, 140, 11, 71, 209, 172, 19, 198, 46, 27,
        0, 0, 0, 7, 240, 0, 0, 0, 88, 49, 3, 147, 181, 28, 1, 26, 25, 116, 166, 43, 56, 78, 42,
        134, 132, 42, 191, 56, 201, 240, 167, 211, 246, 121, 196, 206, 153, 40, 118, 180, 128, 107,
        133, 251, 45, 22, 108, 103, 132, 111, 147, 187, 97, 172, 132, 190, 26, 162, 166, 67, 0,
        135, 8, 27, 255, 255, 255, 255, 255, 255, 255, 255, 27, 127, 255, 255, 255, 255, 255, 255,
        255, 64, 64, 246,
    ];
    let invalid_bytes: &[u8] = b"auefbfd7aNasdjfiA";

    // Check deserialization of valid and invalid parameters
    let des_params: Serialized =
        from_slice(valid_bytes).expect("valid parameter bytes should be able to be deserialized");
    assert!(from_slice::<Serialized>(invalid_bytes).is_err());

    // Check to make sure bytes can be serialized
    let enc_params = to_vec(&des_params).unwrap();

    // Assert symmetric serialization
    assert_eq!(enc_params.as_slice(), valid_bytes);
}

#[test]
fn cbor_params() {
    // Test cbor encodable objects can be added and removed from parameters
    let addr = Address::new_id(1);
    let params = Serialized::serialize(&addr).unwrap();
    assert_eq!(from_slice::<Address>(&params).unwrap(), addr);
}
