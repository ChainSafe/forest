// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::{proto, GraphSyncMessage};
use bytes::{Bytes, BytesMut};
use futures_codec::{Decoder, Encoder};
use protobuf::{parse_from_bytes, Message};
use std::convert::TryFrom;
use std::io;
use unsigned_varint::codec;

#[allow(dead_code)]
/// Codec used for encoding and decoding protobuf messages
pub struct GraphSyncCodec {
    pub(crate) length_codec: codec::UviBytes,
}

impl Encoder for GraphSyncCodec {
    type Error = io::Error;
    type Item = GraphSyncMessage;

    fn encode(&mut self, item: Self::Item, dst: &mut BytesMut) -> Result<(), Self::Error> {
        let proto_msg = proto::Message::try_from(item)?;
        let buf: Vec<u8> = proto_msg.write_to_bytes()?;

        self.length_codec.encode(Bytes::from(buf), dst)
    }
}

impl Decoder for GraphSyncCodec {
    type Error = io::Error;
    type Item = GraphSyncMessage;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        let packet = match self.length_codec.decode(src)? {
            Some(p) => p,
            None => return Ok(None),
        };

        let decoded_packet = parse_from_bytes::<proto::Message>(&packet)?;
        Ok(Some(GraphSyncMessage::try_from(decoded_packet)?))
    }
}
