// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::blocks::Ticket;
use crate::blocks::VRFProof;
use crate::message::SignedMessage;
use crate::shim::{
    address::Address,
    crypto::Signature,
    message::{Message, Message_v3},
};
use base64::{prelude::BASE64_STANDARD, Engine};

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
