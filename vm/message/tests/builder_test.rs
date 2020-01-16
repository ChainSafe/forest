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
    assert_eq!(message.from().clone(), from_addr.clone());
    assert_eq!(message.to().clone(), to_addr.clone());
    assert_eq!(message.sequence(), 0);
    assert_eq!(message.method_num().clone(), MethodNum::default());
    assert_eq!(message.params().clone(), MethodParams::default());
    assert_eq!(message.value().clone(), TokenAmount::new(0));
    assert_eq!(message.gas_price().clone(), BigUint::default());
    assert_eq!(message.gas_limit().clone(), BigUint::default());
    let mut mb = UnsignedMessage::builder();
    mb.to(to_addr.clone());
    mb.from(from_addr.clone());
    {
        // Test scoped modification still applies to builder
        mb.sequence(1);
    }
    // test unwrapping
    let u_msg = mb.build().unwrap();
    assert_eq!(u_msg.from().clone(), from_addr.clone());
    assert_eq!(u_msg.to().clone(), to_addr.clone());
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
    assert_eq!(signed_msg.message().clone(), unsigned_msg);
    assert_eq!(signed_msg.signature().clone(), DUMMY_SIG.to_vec());
}
