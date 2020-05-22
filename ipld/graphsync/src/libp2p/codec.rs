// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::RPCError;
use crate::GraphSyncMessage;
use bytes::BytesMut;
use futures_codec::{Decoder, Encoder};
use unsigned_varint::codec;

#[allow(dead_code)]
/// Codec used
pub struct GraphSyncCodec {
    pub(crate) length_codec: codec::UviBytes,
}

impl Encoder for GraphSyncCodec {
    type Error = RPCError;
    type Item = GraphSyncMessage;

    fn encode(&mut self, _item: Self::Item, _dst: &mut BytesMut) -> Result<(), Self::Error> {
        todo!()
    }
}

impl Decoder for GraphSyncCodec {
    type Error = RPCError;
    type Item = GraphSyncMessage;

    fn decode(&mut self, _bz: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        todo!()
    }
}
