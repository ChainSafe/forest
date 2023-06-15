// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use forest_message::SignedMessage;
use forest_shim::{address::Address, crypto::Signature, message::Message};
use rand::{rngs::OsRng, RngCore};

#[test]
fn generate_signed_message() {
    let msg = Message::new(
        Default::default(),
        Address::new_id(2).into(),
        Address::new_id(1).into(),
        Default::default(),
        Default::default(),
        Default::default(),
        Default::default(),
        Default::default(),
        Default::default(),
        Default::default(),
    );

    let mut dummy_sig = vec![0];
    OsRng.fill_bytes(&mut dummy_sig);
    let signed_msg =
        SignedMessage::new_unchecked(msg.clone(), Signature::new_secp256k1(dummy_sig.clone()));

    // Assert message and signature are expected
    assert_eq!(signed_msg.message(), &msg);
    assert_eq!(signed_msg.signature(), &Signature::new_secp256k1(dummy_sig));
}
