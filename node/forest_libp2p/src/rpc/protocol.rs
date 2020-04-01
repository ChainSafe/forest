// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{InboundCodec, OutboundCodec, RPCError};
use crate::blocksync::{BlockSyncRequest, BlockSyncResponse, BLOCKSYNC_PROTOCOL_ID};
use crate::hello::{HelloMessage, HelloResponse, HELLO_PROTOCOL_ID};
use bytes::BytesMut;
use futures::prelude::*;
use futures::{AsyncRead, AsyncReadExt, AsyncWrite};
use futures_codec::{Decoder, Encoder, Framed};
use libp2p::core::UpgradeInfo;
use libp2p::swarm::NegotiatedSubstream;
use libp2p::{InboundUpgrade, OutboundUpgrade};
use std::pin::Pin;

/// RPCResponse payloads for request/response calls
#[derive(Debug, Clone, PartialEq)]
pub enum RPCResponse {
    BlockSync(BlockSyncResponse),
    Hello(HelloResponse),
}

/// Protocol upgrade for inbound RPC requests.
#[derive(Debug, Clone)]
pub struct RPCInbound;

impl UpgradeInfo for RPCInbound {
    type Info = &'static [u8];
    type InfoIter = Vec<Self::Info>;

    fn protocol_info(&self) -> Self::InfoIter {
        vec![BLOCKSYNC_PROTOCOL_ID, HELLO_PROTOCOL_ID]
    }
}

pub(crate) type InboundFramed<TSocket> = Framed<TSocket, InboundCodec>;
pub(crate) type InboundOutput<TSocket> = (RPCRequest, InboundFramed<TSocket>);

impl<TSocket> InboundUpgrade<TSocket> for RPCInbound
where
    TSocket: AsyncWrite + AsyncRead + Unpin + Send + 'static,
{
    type Output = InboundOutput<TSocket>;
    type Error = RPCError;
    #[allow(clippy::type_complexity)]
    type Future = Pin<Box<dyn Future<Output = Result<Self::Output, Self::Error>> + Send>>;

    fn upgrade_inbound(self, mut socket: TSocket, protocol: Self::Info) -> Self::Future {
        Box::pin(async move {
            let mut buf = Vec::default();
            socket.read_to_end(&mut buf).await?;
            let mut bm = BytesMut::from(&buf[..]);
            let mut codec = InboundCodec::new(protocol);
            let req = codec.decode(&mut bm)?.unwrap();
            Ok((req, Framed::new(socket, codec)))
        })
    }
}

/// RPCRequest payloads for request/response calls
#[derive(Debug, Clone, PartialEq)]
pub enum RPCRequest {
    BlockSync(BlockSyncRequest),
    Hello(HelloMessage),
}

impl UpgradeInfo for RPCRequest {
    type Info = &'static [u8];
    type InfoIter = Vec<Self::Info>;

    fn protocol_info(&self) -> Self::InfoIter {
        self.supported_protocols()
    }
}

impl RPCRequest {
    pub fn supported_protocols(&self) -> Vec<&'static [u8]> {
        match self {
            RPCRequest::BlockSync(_) => vec![BLOCKSYNC_PROTOCOL_ID],
            RPCRequest::Hello(_) => vec![HELLO_PROTOCOL_ID],
        }
    }
    pub fn expect_response(&self) -> bool {
        match self {
            RPCRequest::BlockSync(_) => true,
            RPCRequest::Hello(_) => true,
        }
    }
}

pub(crate) type OutboundFramed = Framed<NegotiatedSubstream, OutboundCodec>;

impl<TSocket> OutboundUpgrade<TSocket> for RPCRequest
where
    TSocket: AsyncWrite + AsyncRead + Unpin + Send + 'static,
{
    type Output = Framed<TSocket, OutboundCodec>;
    type Error = RPCError;
    #[allow(clippy::type_complexity)]
    type Future = Pin<Box<dyn Future<Output = Result<Self::Output, Self::Error>> + Send>>;

    fn upgrade_outbound(self, mut socket: TSocket, protocol: Self::Info) -> Self::Future {
        Box::pin(async move {
            let mut bm = BytesMut::with_capacity(1024);
            let mut codec = OutboundCodec::new(protocol);
            codec.encode(self, &mut bm)?;
            socket.write_all(&bm).await?;
            socket.close().await?;
            Ok(Framed::new(socket, codec))
        })
    }
}
