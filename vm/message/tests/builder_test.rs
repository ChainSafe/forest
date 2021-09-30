// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use address::Address;
use crypto::{Signature, Signer};
use forest_message::{Message, SignedMessage, UnsignedMessage};
use std::error::Error;
use vm::{MethodNum, Serialized, TokenAmount};

const DUMMY_SIG: [u8; 1] = [0u8];

struct DummySigner;
impl Signer for DummySigner {
    fn sign_bytes(&self, _: &[u8], _: &Address) -> Result<Signature, Box<dyn Error>> {
        Ok(Signature::new_secp256k1(DUMMY_SIG.to_vec()))
    }
}

#[test]
fn unsigned_message_builder() {
    let to_addr = Address::new_id(1);
    let from_addr = Address::new_id(2);
    // Able to build with chaining just to and from fields
    let message = UnsignedMessage::builder()
        .to(to_addr)
        .from(from_addr)
        .sequence(0)
        .value(TokenAmount::from(0u8))
        .method_num(MethodNum::default())
        .params(Serialized::default())
        .gas_limit(0)
        .gas_premium(TokenAmount::from(0u8))
        .build()
        .unwrap();
    assert_eq!(message.from(), &from_addr);
    assert_eq!(message.to(), &to_addr);
    assert_eq!(message.sequence(), 0);
    assert_eq!(message.method_num(), MethodNum::default());
    assert_eq!(message.params(), &Serialized::default());
    assert_eq!(message.value(), &TokenAmount::from(0u8));
    assert_eq!(message.gas_premium(), &TokenAmount::from(0u8));
    assert_eq!(message.gas_limit(), 0);
    let mut mb = UnsignedMessage::builder();
    mb.to(to_addr);
    mb.from(from_addr);
    {
        // Test scoped modification still applies to builder
        mb.sequence(1);
    }
    // test unwrapping
    let u_msg = mb.build().unwrap();
    assert_eq!(u_msg.from(), &from_addr);
    assert_eq!(u_msg.to(), &to_addr);
    assert_eq!(u_msg.sequence(), 1);
}

#[test]
fn generate_signed_message() {
    let unsigned_msg = UnsignedMessage::builder()
        .to(Address::new_id(1))
        .from(Address::new_id(2))
        .build()
        .unwrap();

    let signed_msg = SignedMessage::new(unsigned_msg.clone(), &DummySigner).unwrap();

    // Assert message and signature are expected
    assert_eq!(signed_msg.message(), &unsigned_msg);
    assert_eq!(
        signed_msg.signature(),
        &Signature::new_secp256k1(DUMMY_SIG.to_vec())
    );
}
