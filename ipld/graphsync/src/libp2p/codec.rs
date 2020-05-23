// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::GraphSyncMessage;
use bytes::BytesMut;
use futures_codec::{Decoder, Encoder};
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

    fn encode(&mut self, _item: Self::Item, _dst: &mut BytesMut) -> Result<(), Self::Error> {
        todo!()
    }
}

impl Decoder for GraphSyncCodec {
    type Error = io::Error;
    type Item = GraphSyncMessage;

    fn decode(&mut self, _bz: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        todo!()
    }
}
