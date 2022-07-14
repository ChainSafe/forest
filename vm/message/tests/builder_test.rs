// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use forest_address::Address;
use forest_crypto::{Signature, Signer};
use forest_message::SignedMessage;
use fvm_shared::message::Message;

const DUMMY_SIG: [u8; 1] = [0u8];

struct DummySigner;
impl Signer for DummySigner {
    fn sign_bytes(&self, _: &[u8], _: &Address) -> Result<Signature, anyhow::Error> {
        Ok(Signature::new_secp256k1(DUMMY_SIG.to_vec()))
    }
}

#[test]
fn generate_signed_message() {
    let msg = Message {
        to: Address::new_id(1),
        from: Address::new_id(2),
        ..Message::default()
    };

    let signed_msg = SignedMessage::new(msg.clone(), &DummySigner).unwrap();

    // Assert message and signature are expected
    assert_eq!(signed_msg.message(), &msg);
    assert_eq!(
        signed_msg.signature(),
        &Signature::new_secp256k1(DUMMY_SIG.to_vec())
    );
}
