// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

// Doesn't run these unless feature specified
#![cfg(feature = "submodule_tests")]

use std::{fs::File, io::prelude::*, str::FromStr};

use base64::{prelude::BASE64_STANDARD, Engine};
use bls_signatures::{PrivateKey, Serialize};
use cid::Cid;
use forest_json::{message, signature};
use forest_message::signed_message::SignedMessage;
use fvm_ipld_encoding::Cbor;
use fvm_shared::{crypto::signature::Signature, message::Message};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct TestVec {
    #[serde(with = "message::json")]
    unsigned: Message,
    cid: String,
    private_key: String,
    #[serde(with = "signature::json")]
    signature: Signature,
}

#[test]
fn signing_test() {
    let mut file = File::open("serialization-vectors/message_signing.json").unwrap();
    let mut string = String::new();
    file.read_to_string(&mut string).unwrap();

    let vectors: Vec<TestVec> =
        serde_json::from_str(&string).expect("Test vector deserialization failed");
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
