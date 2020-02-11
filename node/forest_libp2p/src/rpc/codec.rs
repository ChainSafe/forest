// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::rpc_message::{RPCRequest, RPCResponse};
use bytes::BytesMut;
use forest_encoding::{error::Error as EncodingError, from_slice, to_vec};
use futures_codec::{Decoder, Encoder};
use std::fmt;

pub struct InboundCodec;
pub struct OutboundCodec;

#[derive(Debug, Clone)]
pub enum RPCError {
    Codec,
    Custom(String),
}
impl From<std::io::Error> for RPCError {
    fn from(err: std::io::Error) -> Self {
        Self::Custom(err.to_string())
    }
}

impl From<EncodingError> for RPCError {
    fn from(_: EncodingError) -> Self {
        Self::Codec
    }
}

impl fmt::Display for RPCError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RPCError::Codec => write!(f, "Codec Error"),
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
            RPCResponse::BlocksyncResponse(response) => {
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
        Ok(Some(RPCRequest::BlocksyncRequest(
            // Reaplce map
            from_slice(bz).map_err(|err| {
                println!("InboundCodec decode ERR: {}", err);
                RPCError::Codec
            })?,
        )))
    }
}

impl Encoder for OutboundCodec {
    type Error = RPCError;
    type Item = RPCRequest;

    fn encode(&mut self, item: Self::Item, dst: &mut BytesMut) -> Result<(), Self::Error> {
        match item {
            RPCRequest::BlocksyncRequest(request) => {
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
        Ok(Some(RPCResponse::BlocksyncResponse(
            // Reaplce map
            from_slice(bz).map_err(|err| {
                println!("OutboundCodec decode ERR: {}", err);
                RPCError::Codec
            })?,
        )))
    }
}
