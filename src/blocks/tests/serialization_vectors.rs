// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

/// These tests use the `serialization-vectors` submodule at the root of this repo
use crate::blocks::BlockHeader;
use crate::message::signed_message::SignedMessage;
use crate::shim::{crypto::Signature, message::Message};
use bls_signatures::{PrivateKey, Serialize as _};
use cid::Cid;
use serde::Deserialize;

#[test]
fn header_cbor_vectors() {
    #[derive(Deserialize)]
    struct Case {
        #[serde(with = "crate::lotus_json")]
        block: BlockHeader,
        #[serde(with = "hex")]
        cbor_hex: Vec<u8>,
        #[serde(with = "crate::lotus_json::stringify")] // yes this isn't CidLotusJson...
        cid: Cid,
    }

    let s = include_str!("../../../serialization-vectors/block_headers.json");

    let cases: Vec<Case> = serde_json::from_str(s).expect("Test vector deserialization failed");

    for Case {
        block,
        cbor_hex,
        cid,
    } in cases
    {
        assert_eq!(cbor_hex, fvm_ipld_encoding::to_vec(&block).unwrap());
        assert_eq!(*block.cid(), cid);
    }
}

#[test]
fn signing_test() {
    #[derive(Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct Case {
        #[serde(with = "crate::lotus_json")]
        unsigned: Message,
        #[serde(with = "crate::lotus_json::stringify")] // yes this isn't CidLotusJson...
        cid: Cid,
        #[serde(with = "crate::lotus_json::base64_standard")]
        private_key: Vec<u8>,
        #[serde(with = "crate::lotus_json")]
        signature: Signature,
    }

    let s = include_str!("../../../serialization-vectors/message_signing.json");

    let cases: Vec<Case> = serde_json::from_str(s).expect("Test vector deserialization failed");

    for Case {
        unsigned,
        cid: expected_cid,
        private_key,
        signature,
    } in cases
    {
        // TODO set up a private key based on sig type
        let priv_key = PrivateKey::from_bytes(&private_key).unwrap();
        let msg_sign_bz = unsigned.cid().unwrap().to_bytes();
        let bls_sig = priv_key.sign(&msg_sign_bz);
        let sig = Signature::new_bls(bls_sig.as_bytes());
        assert_eq!(sig, signature);

        let smsg = SignedMessage::new_from_parts(unsigned, sig).unwrap();
        let actual_cid = smsg.cid().unwrap();

        assert_eq!(actual_cid, expected_cid);
    }
}

#[test]
fn unsigned_message_cbor_vectors() {
    #[derive(Deserialize)]
    struct Case {
        #[serde(with = "crate::lotus_json")]
        message: Message,
        #[serde(with = "hex")]
        hex_cbor: Vec<u8>,
    }

    let s = include_str!("../../../serialization-vectors/unsigned_messages.json");

    let vectors: Vec<Case> = serde_json::from_str(s).expect("Test vector deserialization failed");
    for Case {
        message,
        hex_cbor: expected_cbor,
    } in vectors
    {
        let actual_cbor: Vec<u8> = fvm_ipld_encoding::to_vec(&message).unwrap();
        assert_eq!(expected_cbor, actual_cbor);
    }
}
