// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

// Doesn't run these unless feature specified
#![cfg(feature = "submodule_tests")]

use encoding::{from_slice, to_vec};
use forest_message::UnsignedMessage;
use hex::encode;
use serde::Deserialize;
use std::fs::File;
use std::io::prelude::*;

mod unsigned_message_json {
    use super::UnsignedMessage;
    use serde::{de, Deserialize, Deserializer};
    use vm::Serialized;

    pub fn deserialize<'de, D>(deserializer: D) -> Result<UnsignedMessage, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct UnsignedMessageDe {
            #[serde(alias = "Version")]
            version: i64,
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
        let m: UnsignedMessageDe = Deserialize::deserialize(deserializer)?;
        UnsignedMessage::builder()
            .version(m.version)
            .to(m.to.parse().map_err(de::Error::custom)?)
            .from(m.from.parse().map_err(de::Error::custom)?)
            .sequence(m.nonce)
            .value(m.value.parse().map_err(de::Error::custom)?)
            .method_num(m.method)
            .params(Serialized::new(
                base64::decode(&m.params).map_err(de::Error::custom)?,
            ))
            .gas_limit(m.gas_limit)
            .gas_price(m.gas_price.parse().map_err(de::Error::custom)?)
            .build()
            .map_err(de::Error::custom)
    }
}

#[derive(Deserialize)]
struct TestVector {
    #[serde(with = "unsigned_message_json")]
    message: UnsignedMessage,
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
    for TestVector { message, hex_cbor } in vectors {
        encode_assert_cbor(&message, &hex_cbor)
    }
}
