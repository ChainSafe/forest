// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

// Doesn't run these unless feature specified
#![cfg(feature = "submodule_tests")]

use std::{fs::File, io::prelude::*};

use forest_json::message;
use forest_shim::{
    address::{set_current_network, Network},
    message::Message,
};
use fvm_ipld_encoding::to_vec;
use hex::encode;
use serde::Deserialize;

#[derive(Deserialize)]
struct TestVector {
    #[serde(with = "message::json")]
    message: Message,
    hex_cbor: String,
}

fn encode_assert_cbor(message: &Message, expected: &str) {
    let enc_bz: Vec<u8> = to_vec(message).expect("Cbor serialization failed");

    // Assert the message is encoded in same format
    assert_eq!(encode(enc_bz.as_slice()), expected);
}

#[test]
fn unsigned_message_cbor_vectors() {
    set_current_network(Network::Testnet);

    let mut file = File::open("serialization-vectors/unsigned_messages.json").unwrap();
    let mut string = String::new();
    file.read_to_string(&mut string).unwrap();

    let vectors: Vec<TestVector> =
        serde_json::from_str(&string).expect("Test vector deserialization failed");
    for TestVector { message, hex_cbor } in vectors {
        encode_assert_cbor(&message, &hex_cbor)
    }
}
