// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::message::SignedMessage;
use crate::shim::{
    address::Address,
    crypto::Signature,
    message::{Message, Message_v3},
};
use rand::{rngs::OsRng, RngCore};

#[test]
fn generate_signed_message() {
    let msg: Message = Message_v3 {
        to: Address::new_id(1).into(),
        from: Address::new_id(2).into(),
        ..Message_v3::default()
    }
    .into();

    let mut dummy_sig = vec![0];
    OsRng.fill_bytes(&mut dummy_sig);
    let signed_msg =
        SignedMessage::new_unchecked(msg.clone(), Signature::new_secp256k1(dummy_sig.clone()));

    // Assert message and signature are expected
    assert_eq!(signed_msg.message(), &msg);
    assert_eq!(signed_msg.signature(), &Signature::new_secp256k1(dummy_sig));
}
