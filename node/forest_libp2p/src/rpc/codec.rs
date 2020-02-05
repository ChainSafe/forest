use super::rpc_message::{RPCRequest, RPCResponse};
use bytes::BytesMut;
use forest_encoding::{from_slice, to_vec};
use futures_codec::{Decoder, Encoder};

pub struct InboundCodec;
pub struct OutboundCodec;

pub struct RPCError;
impl From<std::io::Error> for RPCError {
    fn from(_: std::io::Error) -> Self {
        Self
    }
}

impl Encoder for InboundCodec {
    type Item = RPCResponse;
    type Error = RPCError;

    fn encode(&mut self, item: Self::Item, dst: &mut BytesMut) -> Result<(), Self::Error> {
        match item {
            RPCResponse::BlocksyncResponse(response) => {
                let resp = to_vec(&response).unwrap();
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
            from_slice(bz).map_err(|_| RPCError)?,
        )))
    }
}

impl Encoder for OutboundCodec {
    type Error = RPCError;
    type Item = RPCRequest;

    fn encode(&mut self, item: Self::Item, dst: &mut BytesMut) -> Result<(), Self::Error> {
        match item {
            RPCRequest::BlocksyncRequest(request) => {
                let resp = to_vec(&request).unwrap();
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
            from_slice(bz).map_err(|_| RPCError)?,
        )))
    }
}
