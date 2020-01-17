// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

use address::Address;
use crypto::{Signature, Signer};
use message::{Message, SignedMessage, UnsignedMessage};
use num_bigint::BigUint;
use std::error::Error;
use vm::{MethodNum, MethodParams, TokenAmount};

const DUMMY_SIG: [u8; 1] = [0u8];

struct DummySigner;
impl Signer for DummySigner {
    fn sign_bytes(&self, _: Vec<u8>, _: &Address) -> Result<Signature, Box<dyn Error>> {
        Ok(DUMMY_SIG.to_vec())
    }
}

#[test]
fn unsigned_message_builder() {
    let to_addr = Address::new_id(1).unwrap();
    let from_addr = Address::new_id(2).unwrap();
    // Able to build with chaining just to and from fields
    let message = UnsignedMessage::builder()
        .to(to_addr.clone())
        .from(from_addr.clone())
        .sequence(0)
        .value(TokenAmount::new(0))
        .method_num(MethodNum::default())
        .params(MethodParams::default())
        .gas_limit(BigUint::default())
        .gas_price(BigUint::default())
        .build()
        .unwrap();
    assert_eq!(message.from(), &from_addr.clone());
    assert_eq!(message.to(), &to_addr.clone());
    assert_eq!(message.sequence(), 0);
    assert_eq!(message.method_num(), &MethodNum::default());
    assert_eq!(message.params(), &MethodParams::default());
    assert_eq!(message.value(), &TokenAmount::new(0));
    assert_eq!(message.gas_price(), &BigUint::default());
    assert_eq!(message.gas_limit(), &BigUint::default());
    let mut mb = UnsignedMessage::builder();
    mb.to(to_addr.clone());
    mb.from(from_addr.clone());
    {
        // Test scoped modification still applies to builder
        mb.sequence(1);
    }
    // test unwrapping
    let u_msg = mb.build().unwrap();
    assert_eq!(u_msg.from(), &from_addr.clone());
    assert_eq!(u_msg.to(), &to_addr.clone());
    assert_eq!(u_msg.sequence(), 1);
}

#[test]
fn generate_signed_message() {
    let unsigned_msg = UnsignedMessage::builder()
        .to(Address::new_id(1).unwrap())
        .from(Address::new_id(2).unwrap())
        .build()
        .unwrap();

    let signed_msg = SignedMessage::new(&unsigned_msg, &DummySigner).unwrap();

    // Assert message and signature are expected
    assert_eq!(signed_msg.message(), &unsigned_msg);
    assert_eq!(signed_msg.signature(), &DUMMY_SIG.to_vec());
}
