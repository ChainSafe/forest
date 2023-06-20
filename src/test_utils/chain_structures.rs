// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::blocks::Ticket;
use crate::json::vrf::VRFProof;
use crate::message::SignedMessage;
use crate::shim::{
    address::Address,
    crypto::Signature,
    message::{Message, Message_v3},
};
use base64::{prelude::BASE64_STANDARD, Engine};

/// Returns a Ticket to be used for testing
#[allow(unused)] // TODO(aatifsyed)
pub fn construct_ticket() -> Ticket {
    let vrf_result = VRFProof::new(BASE64_STANDARD.decode("lmRJLzDpuVA7cUELHTguK9SFf+IVOaySG8t/0IbVeHHm3VwxzSNhi1JStix7REw6Apu6rcJQV1aBBkd39gQGxP8Abzj8YXH+RdSD5RV50OJHi35f3ixR0uhkY6+G08vV").unwrap());
    Ticket::new(vrf_result)
}

/// Returns a tuple of unsigned and signed messages used for testing
#[allow(unused)] // TODO(aatifsyed)
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
