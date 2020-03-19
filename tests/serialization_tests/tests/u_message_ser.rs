// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

// Doesn't run these unless feature specified
#![cfg(feature = "serde_tests")]

use address::Address;
use encoding::{from_slice, to_vec};
use forest_message::UnsignedMessage;
use hex::encode;
use serde::Deserialize;
use std::fs::File;
use std::io::prelude::*;
use std::str::FromStr;
use vm::{MethodNum, Serialized};

#[derive(Debug, Deserialize)]
struct MessageVector {
    #[serde(alias = "To")]
    to: String,
    #[serde(alias = "From")]
    from: String,
    #[serde(alias = "Nonce")]
    nonce: u64,
    #[serde(alias = "Value")]
    value: String,
    #[serde(alias = "GasPrice")]
    gas_price: String,
    #[serde(alias = "GasLimit")]
    gas_limit: u64,
    #[serde(alias = "Method")]
    method: u64,
    #[serde(alias = "Params")]
    params: String,
}

impl From<MessageVector> for UnsignedMessage {
    fn from(vector: MessageVector) -> UnsignedMessage {
        UnsignedMessage::builder()
            .to(Address::from_str(&vector.to).unwrap())
            .from(Address::from_str(&vector.from).unwrap())
            .sequence(vector.nonce)
            .value(vector.value.parse().unwrap())
            .method_num(MethodNum::new(vector.method))
            .params(Serialized::new(base64::decode(&vector.params).unwrap()))
            .gas_limit(vector.gas_limit)
            .gas_price(vector.gas_price.parse().unwrap())
            .build()
            .unwrap()
    }
}

#[derive(Deserialize)]
struct TestVector {
    message: MessageVector,
    hex_cbor: String,
}

fn encode_assert_cbor(message: &UnsignedMessage, expected: &str) {
    let enc_bz: Vec<u8> = to_vec(message).expect("Cbor serialization failed");

    // Assert the message is encoded in same format
    assert_eq!(encode(enc_bz.as_slice()), expected);
    // Assert decoding from those bytes goes back to unsigned message
    assert_eq!(
        &from_slice::<UnsignedMessage>(&enc_bz).expect("Should be able to deserialize cbor bytes"),
        message
    );
}

#[test]
fn unsigned_message_cbor_vectors() {
    let mut file = File::open("../serialization-vectors/unsigned_messages.json").unwrap();
    let mut string = String::new();
    file.read_to_string(&mut string).unwrap();

    let vectors: Vec<TestVector> =
        serde_json::from_str(&string).expect("Test vector deserialization failed");
    for tv in vectors {
        encode_assert_cbor(&UnsignedMessage::from(tv.message), &tv.hex_cbor)
    }
}
