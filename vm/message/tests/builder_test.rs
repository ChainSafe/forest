// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use forest_test_utils::*;
use fvm_shared::address::Address;
use fvm_shared::crypto::signature::Signature;
use fvm_shared::message::Message;

#[test]
fn generate_signed_message() {
    let msg = Message {
        to: Address::new_id(1),
        from: Address::new_id(2),
        ..Message::default()
    };

    let signed_msg = DummySigner::sign_message(msg.clone()).unwrap();

    // Assert message and signature are expected
    assert_eq!(signed_msg.message(), &msg);
    assert_eq!(
        signed_msg.signature(),
        &Signature::new_secp256k1(DUMMY_SIG.to_vec())
    );
}
