// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

// Doesn't run these unless feature specified
#![cfg(feature = "submodule_tests")]
#![allow(dead_code, non_snake_case)]

use bls_signatures::{PrivateKey, Serialize};
use crypto::{signature, Signature};
use encoding::Cbor;
use forest_message::{unsigned_message, UnsignedMessage};
use serde::Deserialize;
use std::fs::File;
use std::io::prelude::*;

#[derive(Deserialize)]
struct TestVec {
    #[serde(with = "unsigned_message::json")]
    Unsigned: UnsignedMessage,
    Cid: String,
    CidHexBytes: String,
    PrivateKey: String,
    #[serde(with = "signature::json")]
    Signature: Signature,
}

#[test]
fn signing_test() {
    let mut file = File::open("../serialization-vectors/message_signing.json").unwrap();
    let mut string = String::new();
    file.read_to_string(&mut string).unwrap();

    let vectors: Vec<TestVec> =
        serde_json::from_str(&string).expect("Test vector deserialization failed");
    for test_vec in vectors {
        let test = base64::decode(test_vec.PrivateKey).unwrap();
        let priv_key = PrivateKey::from_bytes(&test).unwrap();
        let cid = test_vec.Unsigned.cid().unwrap();
        let sig = priv_key.sign(cid.to_bytes().as_slice());
        assert_eq!(sig.as_bytes().as_slice(), test_vec.Signature.bytes());
    }
}
