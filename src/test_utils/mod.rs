// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::str::FromStr;

use crate::{
    blocks::{Ticket, VRFProof},
    eth::EVMMethod,
    message::SignedMessage,
    shim::{
        address::Address,
        crypto::{SECP_SIG_LEN, Signature, SignatureType},
        message::{Message, Message_v3},
    },
};
use base64::{Engine, prelude::BASE64_STANDARD};

/// Returns a Ticket to be used for testing
pub fn construct_ticket() -> Ticket {
    let vrf_result = VRFProof::new(BASE64_STANDARD.decode("lmRJLzDpuVA7cUELHTguK9SFf+IVOaySG8t/0IbVeHHm3VwxzSNhi1JStix7REw6Apu6rcJQV1aBBkd39gQGxP8Abzj8YXH+RdSD5RV50OJHi35f3ixR0uhkY6+G08vV").unwrap());
    Ticket::new(vrf_result)
}

/// Returns a tuple of unsigned and signed messages used for testing
pub fn construct_messages() -> (Message, SignedMessage) {
    let bls_messages: Message = Message_v3 {
        to: Address::new_id(1).into(),
        from: Address::new_id(2).into(),
        ..Message_v3::default()
    }
    .into();

    let secp_messages =
        SignedMessage::new_unchecked(bls_messages.clone(), Signature::new_secp256k1(vec![0]));
    (bls_messages, secp_messages)
}

/// Returns a tuple of unsigned and BLS-signed messages used for testing
pub fn construct_bls_messages() -> (Message, SignedMessage) {
    let message: Message = Message_v3 {
        to: Address::new_id(1).into(),
        from: Address::new_id(2).into(),
        ..Message_v3::default()
    }
    .into();

    let bls_message = SignedMessage::new_unchecked(message.clone(), Signature::new_bls(vec![0]));
    (message, bls_message)
}

/// Returns a tuple of unsigned and signed messages used for testing the Ethereum mapping
pub fn construct_eth_messages(sequence: u64) -> (Message, SignedMessage) {
    let mut eth_message: Message = Message_v3 {
        to: Address::from_str("t410foy6ucbmuujaequ3zsdo6nsubyogp6vtk23t4odq")
            .unwrap()
            .into(),
        from: Address::from_str("t410fse4uvumo6ko46igb6lshg3peztqs3h6755vommy")
            .unwrap()
            .into(),
        ..Message_v3::default()
    }
    .into();
    eth_message.method_num = EVMMethod::InvokeContract as u64;
    eth_message.sequence = sequence;

    let secp_message = SignedMessage::new_unchecked(
        eth_message.clone(),
        Signature::new(SignatureType::Delegated, vec![0; SECP_SIG_LEN]),
    );

    (eth_message, secp_message)
}

// Serialize macro used for testing
#[macro_export]
macro_rules! to_string_with {
    ($obj:expr, $serializer:path) => {{
        let mut writer = Vec::new();
        $serializer($obj, &mut serde_json::ser::Serializer::new(&mut writer)).unwrap();
        String::from_utf8(writer).unwrap()
    }};
}

// Deserialize macro used for testing
#[macro_export]
macro_rules! from_str_with {
    ($str:expr, $deserializer:path) => {
        $deserializer(&mut serde_json::de::Deserializer::from_str($str)).unwrap()
    };
}
