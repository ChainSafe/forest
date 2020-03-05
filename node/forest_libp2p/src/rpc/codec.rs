// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{RPCError, RPCRequest, RPCResponse};
use crate::blocksync::BLOCKSYNC_PROTOCOL_ID;
use crate::hello::HELLO_PROTOCOL_ID;
use bytes::BytesMut;
use forest_encoding::{from_slice, to_vec};
use futures_codec::{Decoder, Encoder};

/// Codec used for inbound connections. Decodes the inbound message into a RPCRequest, and encodes the RPCResponse to send.
pub struct InboundCodec {
    protocol: &'static [u8],
}

impl InboundCodec {
    pub fn new(protocol: &'static [u8]) -> Self {
        Self { protocol }
    }
}

impl Encoder for InboundCodec {
    type Error = RPCError;
    type Item = RPCResponse;

    fn encode(&mut self, item: Self::Item, dst: &mut BytesMut) -> Result<(), Self::Error> {
        match item {
            RPCResponse::BlockSync(response) => {
                let resp = to_vec(&response)?;
                dst.clear();
                dst.extend_from_slice(&resp);
                Ok(())
            }
            RPCResponse::Hello(response) => {
                let resp = to_vec(&response)?;
                dst.clear();
                dst.extend_from_slice(&resp);
                Ok(())
            }
        }
    }
}

impl Decoder for InboundCodec {
    type Error = RPCError;
    type Item = RPCRequest;

    fn decode(&mut self, bz: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if bz.is_empty() {
            return Ok(None);
        }

        match self.protocol {
            HELLO_PROTOCOL_ID => Ok(Some(RPCRequest::Hello(
                from_slice(bz).map_err(|err| RPCError::Codec(err.to_string()))?,
            ))),
            BLOCKSYNC_PROTOCOL_ID => Ok(Some(RPCRequest::BlockSync(
                from_slice(bz).map_err(|err| RPCError::Codec(err.to_string()))?,
            ))),
            _ => Err(RPCError::Codec("Unsupported codec".to_string())),
        }
    }
}

/// Codec used for outbound connections. Encodes the outbound message into a RPCRequest to send, and decodes the RPCResponse when received.
pub struct OutboundCodec {
    protocol: &'static [u8],
}

impl OutboundCodec {
    pub fn new(protocol: &'static [u8]) -> Self {
        Self { protocol }
    }
}

impl Encoder for OutboundCodec {
    type Error = RPCError;
    type Item = RPCRequest;
    fn encode(&mut self, item: Self::Item, dst: &mut BytesMut) -> Result<(), Self::Error> {
        match item {
            RPCRequest::BlockSync(request) => {
                let resp = to_vec(&request)?;
                dst.clear();
                dst.extend_from_slice(&resp);
                Ok(())
            }
            RPCRequest::Hello(request) => {
                let resp = to_vec(&request)?;
                dst.clear();
                dst.extend_from_slice(&resp);
                Ok(())
            }
        }
    }
}

impl Decoder for OutboundCodec {
    type Error = RPCError;
    type Item = RPCResponse;
    fn decode(&mut self, bz: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if bz.is_empty() {
            return Ok(None);
        }
        match self.protocol {
            HELLO_PROTOCOL_ID => Ok(Some(RPCResponse::Hello(
                from_slice(bz).map_err(|err| RPCError::Codec(err.to_string()))?,
            ))),
            BLOCKSYNC_PROTOCOL_ID => Ok(Some(RPCResponse::BlockSync(
                from_slice(bz).map_err(|err| RPCError::Codec(err.to_string()))?,
            ))),
            _ => Err(RPCError::Codec("Unsupported codec".to_string())),
        }
    }
}
