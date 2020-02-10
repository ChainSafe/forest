// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{InboundCodec, OutboundCodec, RPCError, RPCRequest};
use bytes::BytesMut;
use futures::prelude::*;
use futures::{AsyncRead, AsyncReadExt, AsyncWrite};
use futures_codec::{Decoder, Encoder, Framed};
use libp2p::core::{Negotiated, UpgradeInfo};
use libp2p::{InboundUpgrade, OutboundUpgrade};
use std::pin::Pin;

// const MAX_RPC_SIZE: u64 = 4_194_304;

#[derive(Debug, Clone)]
pub struct RPCProtocol;

impl UpgradeInfo for RPCProtocol {
    type Info = &'static [u8];
    type InfoIter = Vec<Self::Info>;

    fn protocol_info(&self) -> Self::InfoIter {
        vec![b"/fil/sync/blk/0.0.1"]
    }
}

pub type InboundFramed<TSocket> = Framed<TSocket, InboundCodec>;
pub type InboundOutput<TSocket> = (RPCRequest, InboundFramed<TSocket>);

impl<TSocket> InboundUpgrade<TSocket> for RPCProtocol
where
    TSocket: AsyncWrite + AsyncRead + Unpin + Send + 'static,
{
    type Output = InboundOutput<TSocket>;
    type Error = RPCError;
    #[allow(clippy::type_complexity)]
    type Future = Pin<Box<dyn Future<Output = Result<Self::Output, Self::Error>> + Send>>;

    fn upgrade_inbound(self, mut socket: TSocket, _: Self::Info) -> Self::Future {
        Box::pin(async move {
            // TODO possibly can be done automatically somehow
            let mut bm = BytesMut::with_capacity(1024);
            println!("PRE");
            match socket.read(&mut bm).await {
                Err(e) => {
                    println!("ERR {}", e.to_string());
                    return Err(e.into());
                }
                _ => println!("fine"),
            }
            let req = InboundCodec.decode(&mut bm)?.unwrap();
            println!("{:?}", req);
            Ok((req, Framed::new(socket, InboundCodec)))
        })
    }
}

pub struct RPCOutbound {
    pub req: RPCRequest,
}

impl UpgradeInfo for RPCOutbound {
    type Info = &'static [u8];
    type InfoIter = Vec<Self::Info>;

    fn protocol_info(&self) -> Self::InfoIter {
        vec![b"/fil/sync/blk/0.0.1"]
    }
}

pub type OutboundFramed<TSocket> = Framed<Negotiated<TSocket>, OutboundCodec>;

impl<TSocket> OutboundUpgrade<TSocket> for RPCOutbound
where
    TSocket: AsyncWrite + AsyncRead + Unpin + Send + 'static,
{
    type Output = Framed<TSocket, OutboundCodec>;
    type Error = RPCError;
    #[allow(clippy::type_complexity)]
    type Future = Pin<Box<dyn Future<Output = Result<Self::Output, Self::Error>> + Send>>;

    fn upgrade_outbound(self, mut socket: TSocket, _: Self::Info) -> Self::Future {
        Box::pin(async move {
            println!("Writing to socket");
            // TODO possibly can be done automatically somehow
            let mut bm = BytesMut::with_capacity(1024);
            OutboundCodec.encode(self.req, &mut bm)?;
            socket.write(&bm).await?;
            Ok(Framed::new(socket, OutboundCodec))
        })
    }
}
