// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{RPCRequest, RPCResponse};
use bytes::BytesMut;
use forest_encoding::{error::Error as EncodingError, from_slice, to_vec};
use futures_codec::{Decoder, Encoder};
use std::fmt;

/// Codec used for inbound connections. Decodes the inbound message into a RPCRequest, and encodes the RPCResponse to send.
pub struct InboundCodec;
/// Codec used for outbound connections. Encodes the outbound message into a RPCRequest to send, and decodes the RPCResponse when received.
pub struct OutboundCodec;

#[derive(Debug, Clone, PartialEq)]
pub enum RPCError {
    Codec(String),
    Custom(String),
}
impl From<std::io::Error> for RPCError {
    fn from(err: std::io::Error) -> Self {
        Self::Custom(err.to_string())
    }
}

impl From<EncodingError> for RPCError {
    fn from(err: EncodingError) -> Self {
        Self::Codec(err.to_string())
    }
}

impl fmt::Display for RPCError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RPCError::Codec(err) => write!(f, "Codec Error: {}", err),
            RPCError::Custom(err) => write!(f, "{}", err),
        }
    }
}

impl std::error::Error for RPCError {
    fn description(&self) -> &str {
        "Libp2p RPC Error"
    }
}

impl Encoder for InboundCodec {
    type Error = RPCError;
    type Item = RPCResponse;

    fn encode(&mut self, item: Self::Item, dst: &mut BytesMut) -> Result<(), Self::Error> {
        match item {
            RPCResponse::Blocksync(response) => {
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

        Ok(Some(RPCRequest::Blocksync(
            from_slice(bz).map_err(|err| RPCError::Codec(err.to_string()))?,
        )))
    }
}

impl Encoder for OutboundCodec {
    type Error = RPCError;
    type Item = RPCRequest;

    fn encode(&mut self, item: Self::Item, dst: &mut BytesMut) -> Result<(), Self::Error> {
        match item {
            RPCRequest::Blocksync(request) => {
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

        Ok(Some(RPCResponse::Blocksync(
            // Replace map
            from_slice(bz).map_err(|err| RPCError::Codec(err.to_string()))?,
        )))
    }
}
