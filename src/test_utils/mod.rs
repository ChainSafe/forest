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

pub fn arbitrary_multihash(gen: &mut quickcheck::Gen) -> libp2p::multihash::Multihash<64> {
    todo!()
}

pub fn arbitrary_onion3(gen: &mut quickcheck::Gen) -> libp2p::multiaddr::Onion3Addr<'static> {
    todo!()
}

pub fn arbitrary_protocol(gen: &mut quickcheck::Gen) -> libp2p::multiaddr::Protocol<'static> {
    use libp2p::multiaddr::Protocol as P;

    macro_rules! a {
        () => {
            ::quickcheck::Arbitrary::arbitrary(gen)
        };
        (cow_str) => {
            ::std::borrow::Cow::Owned(
                <::std::string::String as ::quickcheck::Arbitrary>::arbitrary(gen),
            )
        };
    }

    let choices = [
        P::Dccp(a!()),
        P::Dns(a!(cow_str)),
        P::Dns4(a!(cow_str)),
        P::Dns6(a!(cow_str)),
        P::Dnsaddr(a!(cow_str)),
        P::Http,
        P::Https,
        P::Ip4(a!()),
        P::Ip6(a!()),
        P::P2pWebRtcDirect,
        P::P2pWebRtcStar,
        P::WebRTCDirect,
        // P::Certhash(arbitrary_multihash(gen)),
        P::P2pWebSocketStar,
        P::Memory(a!()),
        P::Onion(
            std::borrow::Cow::Owned(std::array::from_fn(|_ix| {
                quickcheck::Arbitrary::arbitrary(gen)
            })),
            a!(),
        ),
        // P::Onion3(arbitrary_onion3(gen)),
        P::P2p(libp2p::PeerId::random()),
        P::P2pCircuit,
        P::Quic,
        P::QuicV1,
        P::Sctp(a!()),
        P::Tcp(a!()),
        P::Tls,
        P::Noise,
        P::Udp(a!()),
        P::Udt,
        P::Unix(a!(cow_str)),
        P::Utp,
        P::WebTransport,
        P::Ws(a!(cow_str)),
        P::Wss(a!(cow_str)),
    ];
    gen.choose(&choices).expect("non-empty choices").clone()
}

pub fn arbitrary_multiaddr(gen: &mut quickcheck::Gen) -> libp2p::Multiaddr {
    <Vec<()> as quickcheck::Arbitrary>::arbitrary(gen)
        .into_iter()
        .map(|()| arbitrary_protocol(gen)) // TODO(aatifsyed): this won't shrink properly
        .collect()
}

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
