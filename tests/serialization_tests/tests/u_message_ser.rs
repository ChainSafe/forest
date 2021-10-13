// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

// Doesn't run these unless feature specified
#![cfg(feature = "submodule_tests")]

use encoding::to_vec;
use forest_message::{unsigned_message, UnsignedMessage};
use hex::encode;
use serde::Deserialize;
use std::fs::File;
use std::io::prelude::*;

#[derive(Deserialize)]
struct TestVector {
    #[serde(with = "unsigned_message::json")]
    message: UnsignedMessage,
    hex_cbor: String,
}

fn encode_assert_cbor(message: &UnsignedMessage, expected: &str) {
    let enc_bz: Vec<u8> = to_vec(message).expect("Cbor serialization failed");

    // Assert the message is encoded in same format
    assert_eq!(encode(enc_bz.as_slice()), expected);
}

#[test]
fn unsigned_message_cbor_vectors() {
    let mut file = File::open("serialization-vectors/unsigned_messages.json").unwrap();
    let mut string = String::new();
    file.read_to_string(&mut string).unwrap();

    let vectors: Vec<TestVector> =
        serde_json::from_str(&string).expect("Test vector deserialization failed");
    for TestVector { message, hex_cbor } in vectors {
        encode_assert_cbor(&message, &hex_cbor)
    }
}
