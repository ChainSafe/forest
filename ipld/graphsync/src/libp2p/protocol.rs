// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::GraphSyncCodec;
use futures::prelude::*;
use futures::{AsyncRead, AsyncWrite};
use futures_codec::Framed;
use libp2p::core::UpgradeInfo;
use libp2p::{InboundUpgrade, OutboundUpgrade};
use std::borrow::Cow;
use std::io;
use std::iter;
use std::pin::Pin;
use unsigned_varint::codec;

/// Protocol upgrade for GraphSync requests.
#[derive(Debug, Clone)]
pub struct ProtocolConfig {
    protocol_id: Cow<'static, [u8]>,
    max_transmit_size: usize,
}

impl Default for ProtocolConfig {
    fn default() -> Self {
        Self {
            protocol_id: Cow::Borrowed(b"/ipfs/graphsync/1.0.0"),
            max_transmit_size: 2048,
        }
    }
}

impl ProtocolConfig {
    pub fn new(id: impl Into<Cow<'static, [u8]>>, max_transmit_size: usize) -> Self {
        Self {
            protocol_id: id.into(),
            max_transmit_size,
        }
    }
}

impl UpgradeInfo for ProtocolConfig {
    type Info = Cow<'static, [u8]>;
    type InfoIter = iter::Once<Self::Info>;

    fn protocol_info(&self) -> Self::InfoIter {
        iter::once(self.protocol_id.clone())
    }
}

impl<TSocket> InboundUpgrade<TSocket> for ProtocolConfig
where
    TSocket: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    type Output = Framed<TSocket, GraphSyncCodec>;
    type Error = io::Error;
    #[allow(clippy::type_complexity)]
    type Future = Pin<Box<dyn Future<Output = Result<Self::Output, Self::Error>> + Send>>;

    fn upgrade_inbound(self, socket: TSocket, _: Self::Info) -> Self::Future {
        let mut length_codec = codec::UviBytes::default();
        length_codec.set_max_len(self.max_transmit_size);
        Box::pin(future::ok(Framed::new(
            socket,
            GraphSyncCodec { length_codec },
        )))
    }
}

impl<TSocket> OutboundUpgrade<TSocket> for ProtocolConfig
where
    TSocket: AsyncWrite + AsyncRead + Unpin + Send + 'static,
{
    type Output = Framed<TSocket, GraphSyncCodec>;
    type Error = io::Error;
    #[allow(clippy::type_complexity)]
    type Future = Pin<Box<dyn Future<Output = Result<Self::Output, Self::Error>> + Send>>;

    fn upgrade_outbound(self, socket: TSocket, _: Self::Info) -> Self::Future {
        let mut length_codec = codec::UviBytes::default();
        length_codec.set_max_len(self.max_transmit_size);
        Box::pin(future::ok(Framed::new(
            socket,
            GraphSyncCodec { length_codec },
        )))
    }
}
