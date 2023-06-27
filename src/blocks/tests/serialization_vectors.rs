// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use crate::blocks::{header, BlockHeader};
use crate::json::{message, signature};
use crate::message::signed_message::SignedMessage;
use crate::shim::{crypto::Signature, message::Message};
/// These tests use the `serialization-vectors` submodule at the root of this repo
use base64::{prelude::BASE64_STANDARD, Engine};
use bls_signatures::{PrivateKey, Serialize};
use cid::Cid;
use fvm_ipld_encoding::{to_vec, Cbor};
use hex::encode;
use serde::Deserialize;
use std::str::FromStr as _;

#[test]
fn header_cbor_vectors() {
    #[derive(Deserialize)]
    struct Case {
        #[serde(with = "header::json")]
        block: BlockHeader,
        cbor_hex: String,
        cid: String,
    }

    let s = include_str!("../../../serialization-vectors/block_headers.json");

    let vectors: Vec<Case> = serde_json::from_str(s).expect("Test vector deserialization failed");

    for tv in vectors {
        let header = &tv.block;
        let expected: &str = &tv.cbor_hex;
        let cid = &tv.cid.parse().unwrap();
        let enc_bz: Vec<u8> = to_vec(header).expect("Cbor serialization failed");

        // Assert the header is encoded in same format
        assert_eq!(encode(enc_bz.as_slice()), expected);
        // Assert decoding from those bytes goes back to unsigned header
        assert_eq!(header.cid(), cid);
    }
}

#[test]
fn signing_test() {
    #[derive(Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct Case {
        #[serde(with = "message::json")]
        unsigned: Message,
        cid: String,
        private_key: String,
        #[serde(with = "signature::json")]
        signature: Signature,
    }

    let s = include_str!("../../../serialization-vectors/message_signing.json");

    let vectors: Vec<Case> = serde_json::from_str(s).expect("Test vector deserialization failed");

    for test_vec in vectors {
        let test = BASE64_STANDARD.decode(test_vec.private_key).unwrap();
        // TODO set up a private key based on sig type
        let priv_key = PrivateKey::from_bytes(&test).unwrap();
        let msg_sign_bz = test_vec.unsigned.cid().unwrap().to_bytes();
        let bls_sig = priv_key.sign(&msg_sign_bz);
        let sig = Signature::new_bls(bls_sig.as_bytes());
        assert_eq!(sig, test_vec.signature);

        let smsg = SignedMessage::new_from_parts(test_vec.unsigned, sig).unwrap();
        let cid = smsg.cid().unwrap();

        let cid_test = Cid::from_str(&test_vec.cid).unwrap();

        assert_eq!(cid, cid_test);
    }
}

#[test]
fn unsigned_message_cbor_vectors() {
    #[derive(Deserialize)]
    struct Case {
        #[serde(with = "message::json")]
        message: Message,
        hex_cbor: String,
    }

    let s = include_str!("../../../serialization-vectors/unsigned_messages.json");

    let vectors: Vec<Case> = serde_json::from_str(s).expect("Test vector deserialization failed");
    for Case { message, hex_cbor } in vectors {
        let expected: &str = &hex_cbor;
        let enc_bz: Vec<u8> = to_vec(&message).expect("Cbor serialization failed");

        // Assert the message is encoded in same format
        assert_eq!(encode(enc_bz.as_slice()), expected);
    }
}
