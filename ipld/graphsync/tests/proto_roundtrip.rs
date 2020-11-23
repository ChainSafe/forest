// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use cid::{Code::Blake2b256, Cid};
use forest_ipld::selector::Selector;
use graphsync::{proto, GraphSyncMessage, GraphSyncRequest, GraphSyncResponse, ResponseStatusCode};
use protobuf::{parse_from_bytes, Message};
use std::convert::TryFrom;
use std::error::Error;

fn roundtrip_test(gsm: GraphSyncMessage) -> Result<(), Box<dyn Error>> {
    // Encode to protobuf bytes
    let pbm = proto::Message::try_from(gsm.clone())?;
    let proto_bytes: Vec<u8> = pbm.write_to_bytes()?;

    // Decode to proto type
    let d_pbm = parse_from_bytes::<proto::Message>(&proto_bytes)?;
    assert_eq!(&d_pbm, &pbm);

    // Decode back to original type
    let d_gsm = GraphSyncMessage::try_from(d_pbm)?;
    assert_eq!(d_gsm, gsm);
    Ok(())
}

#[test]
fn empty_message() {
    roundtrip_test(GraphSyncMessage::default()).unwrap();
}

#[test]
fn requests_message() {
    let mut message = GraphSyncMessage::default();
    message.insert_request(GraphSyncRequest::cancel(2));
    message.insert_request(GraphSyncRequest::update(
        3,
        [
            ("test".to_owned(), vec![8u8]),
            ("second".to_owned(), vec![3u8]),
        ]
        .iter()
        .cloned()
        .collect(),
    ));
    message.insert_request(GraphSyncRequest::new(
        3,
        Cid::new_from_cbor(&[1, 2, 3], Blake2b256),
        Selector::Matcher,
        5,
        None,
    ));

    roundtrip_test(message).unwrap();
}

#[test]
fn responses_message() {
    let mut message = GraphSyncMessage::default();
    message.insert_response(GraphSyncResponse::new(
        4,
        ResponseStatusCode::RequestAcknowledged,
        None,
    ));
    message.insert_response(GraphSyncResponse::new(
        6,
        ResponseStatusCode::Other(-1),
        Some(
            [("extension".to_owned(), vec![1u8])]
                .iter()
                .cloned()
                .collect(),
        ),
    ));

    roundtrip_test(message).unwrap();
}

#[test]
fn blocks_message() {
    let mut message = GraphSyncMessage::default();
    let data = vec![6, 5, 4, 8, 0xff];
    message.insert_block(Cid::new_from_cbor(&data, Blake2b256), data);

    roundtrip_test(message).unwrap();
}

#[test]
fn all_message() {
    // TODO revisit this, seems a bit weird that requests and responses can be sent in the same
    // GraphSync message.
    let mut message = GraphSyncMessage::default();
    let data = vec![6, 5, 4, 8, 0xff];
    message.insert_block(Cid::new_from_cbor(&data, Blake2b256), data);
    message.insert_request(GraphSyncRequest::cancel(2));
    message.insert_response(GraphSyncResponse::new(
        4,
        ResponseStatusCode::RequestAcknowledged,
        None,
    ));

    roundtrip_test(message).unwrap();
}
