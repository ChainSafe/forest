// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use libp2p::{
    PeerId,
    gossipsub::{self, MessageAcceptance, MessageId, TopicHash},
};

use crate::{
    blocks::{CachingBlockHeader, GossipBlock, RawBlockHeader},
    libp2p::{
        NetworkEvent, PubsubMessage, PubsubTopic,
        drand::test_utils::assert_verdict,
        service::{Verdict, handle_gossip_event},
    },
    message::SignedMessage,
    shim::{
        address::Address,
        crypto::Signature,
        message::{Message, Message_v3},
    },
    test_utils::dummy_ticket,
};

const NETWORK: &str = "testgossipvalidation";

pub(super) fn gossip_message_event(data: Vec<u8>, topic: TopicHash) -> gossipsub::Event {
    gossipsub::Event::Message {
        propagation_source: PeerId::random(),
        message_id: MessageId::new(b"test"),
        message: gossipsub::Message {
            source: None,
            data,
            sequence_number: None,
            topic,
        },
    }
}

fn gossip_block() -> Vec<u8> {
    fvm_ipld_encoding::to_vec(&GossipBlock {
        header: CachingBlockHeader::new(RawBlockHeader {
            miner_address: Address::new_id(0),
            ticket: dummy_ticket(0),
            ..Default::default()
        }),
        bls_messages: vec![],
        secpk_messages: vec![],
    })
    .unwrap()
}

fn gossip_message() -> Vec<u8> {
    let message: Message = Message_v3 {
        to: Address::new_id(0).into(),
        from: Address::new_id(0).into(),
        ..Message_v3::default()
    }
    .into();
    fvm_ipld_encoding::to_vec(&SignedMessage::new_unchecked(
        message,
        Signature::new_secp256k1(vec![0]),
    ))
    .unwrap()
}

/// Blocks and messages are accepted once decoded; anything else is ignored.
#[rstest::rstest]
#[case::block(
    Some(PubsubTopic::Blocks),
    gossip_block(),
    MessageAcceptance::Accept,
    true
)]
#[case::message(
    Some(PubsubTopic::Messages),
    gossip_message(),
    MessageAcceptance::Accept,
    true
)]
#[case::undecodable_block(Some(PubsubTopic::Blocks), vec![0xff], MessageAcceptance::Ignore, false)]
#[case::undecodable_message(Some(PubsubTopic::Messages), vec![0xff], MessageAcceptance::Ignore, false)]
#[case::unknown_topic(None, gossip_block(), MessageAcceptance::Ignore, false)]
#[tokio::test]
async fn filecoin_gossip_verdicts(
    #[case] kind: Option<PubsubTopic>,
    #[case] data: Vec<u8>,
    #[case] expected: MessageAcceptance,
    #[case] emitted: bool,
) {
    let block_topic = PubsubTopic::Blocks.ident(NETWORK).to_string();
    let msg_topic = PubsubTopic::Messages.ident(NETWORK).to_string();
    let topic = TopicHash::from_raw(match kind {
        Some(kind) => kind.ident(NETWORK).to_string(),
        None => "/unknown/topic".into(),
    });
    let (tx, rx) = flume::unbounded();

    let Verdict {
        acceptance: verdict,
        ..
    } = handle_gossip_event(
        gossip_message_event(data, topic),
        &tx,
        &block_topic,
        &msg_topic,
<<<<<<< HEAD
        None,
=======
>>>>>>> e176fb76c (chore: enable gossipsub validation)
    )
    .await
    .expect("an immediate verdict");

    assert_verdict(&verdict, expected);
    match rx.try_recv() {
        Ok(NetworkEvent::PubsubMessage {
            message: PubsubMessage::Block(_) | PubsubMessage::Message(_),
        }) => assert!(emitted, "nothing should be emitted"),
        Ok(other) => panic!("unexpected event: {other:?}"),
        Err(_) => assert!(!emitted, "a decoded message should be emitted"),
    }
}
