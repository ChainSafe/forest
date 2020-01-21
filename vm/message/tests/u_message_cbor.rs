// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

use address::Address;
use encoding::{from_slice, to_vec};
use hex::decode;
use message::UnsignedMessage;
use num_bigint::BigUint;
use serde::Deserialize;
use std::fs::File;
use std::io::prelude::*;
use std::str::FromStr;
use vm::{MethodNum, Serialized, TokenAmount};

#[derive(Debug, Deserialize)]
struct MessageVector {
    to: String,
    from: String,
    nonce: u64,
    value: u64,
    gas_price: u128,
    gas_limit: u128,
    method: u64,
    params: String,
}

impl From<MessageVector> for UnsignedMessage {
    fn from(vector: MessageVector) -> UnsignedMessage {
        UnsignedMessage::builder()
            .to(Address::from_str(&vector.to).unwrap())
            .from(Address::from_str(&vector.from).unwrap())
            .sequence(vector.nonce)
            .value(TokenAmount::new(vector.value))
            .method_num(MethodNum::new(vector.method))
            .params(Serialized::new(decode(vector.params).unwrap()))
            .gas_limit(BigUint::from(vector.gas_limit))
            .gas_price(BigUint::from(vector.gas_price))
            .build()
            .unwrap()
    }
}

#[derive(Deserialize)]
struct TestVector {
    message: MessageVector,
    hex_cbor: String,
}

fn encode_assert_cbor(message: &UnsignedMessage, expected: &[u8]) {
    let enc_bz: Vec<u8> = to_vec(message).expect("Cbor serialization failed");

    // Assert the message is encoded in same format
    assert_eq!(enc_bz.as_slice(), expected);
    // Assert decoding from those bytes goes back to unsigned message
    assert_eq!(
        &from_slice::<UnsignedMessage>(expected).expect("Should be able to deserialize cbor bytes"),
        message
    );
}

#[test]
fn unsigned_message_cbor_vectors() {
    let mut file = File::open("../../tests/cbor/unsigned_message_vectors.json").unwrap();
    let mut string = String::new();
    file.read_to_string(&mut string).unwrap();

    let vectors: Vec<TestVector> =
        serde_json::from_str(&string).expect("Test vector deserialization failed");
    for tv in vectors {
        encode_assert_cbor(
            &UnsignedMessage::from(tv.message),
            &decode(tv.hex_cbor).expect("Decoding cbor bytes failed"),
        )
    }
}
