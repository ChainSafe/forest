// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

// Doesn't run these unless feature specified
#![cfg(feature = "submodule_tests")]
#![allow(dead_code)]

use bls_signatures::{PrivateKey, Serialize};
use cid::Cid;
use crypto::{signature, Signature};
use encoding::Cbor;
use forest_message::{unsigned_message, SignedMessage, UnsignedMessage};
use serde::Deserialize;
use std::fs::File;
use std::io::prelude::*;
use std::str::FromStr;

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct TestVec {
    #[serde(with = "unsigned_message::json")]
    unsigned: UnsignedMessage,
    cid: String,
    private_key: String,
    #[serde(with = "signature::json")]
    signature: Signature,
}

#[test]
fn signing_test() {
    let mut file = File::open("../serialization-vectors/message_signing.json").unwrap();
    let mut string = String::new();
    file.read_to_string(&mut string).unwrap();

    let vectors: Vec<TestVec> =
        serde_json::from_str(&string).expect("Test vector deserialization failed");
    for test_vec in vectors {
        let test = base64::decode(test_vec.private_key).unwrap();
        // TODO set up a private key based on sig type
        let priv_key = PrivateKey::from_bytes(&test).unwrap();
        let cid = test_vec.unsigned.cid().unwrap();
        let sig = priv_key.sign(cid.to_bytes().as_slice());
        let msg_sig = Signature::new_bls(sig.as_bytes());
        let signed_message = SignedMessage::new_from_parts(test_vec.unsigned, msg_sig);
        let cid = signed_message.cid().unwrap();
        let cid_test = Cid::from_str(&test_vec.cid).unwrap();

        assert_eq!(sig.as_bytes().as_slice(), test_vec.signature.bytes());
        assert_eq!(cid, cid_test);
    }
}
