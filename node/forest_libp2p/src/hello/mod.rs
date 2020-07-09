// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod message;

pub use self::message::*;
use async_trait::async_trait;
use forest_encoding::{from_slice, to_vec};
use futures::prelude::*;
use libp2p::core::ProtocolName;
use libp2p_request_response::RequestResponseCodec;
use std::io;

pub const HELLO_PROTOCOL_ID: &[u8] = b"/fil/hello/1.0.0";

/// Type to satisfy `ProtocolName` interface for BlockSync RPC.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct HelloProtocolName;

impl ProtocolName for HelloProtocolName {
    fn protocol_name(&self) -> &[u8] {
        HELLO_PROTOCOL_ID
    }
}

/// Hello protocol codec to be used within the RPC service.
#[derive(Debug, Clone, Default)]
pub struct HelloCodec;

#[async_trait]
impl RequestResponseCodec for HelloCodec {
    type Protocol = HelloProtocolName;
    type Request = HelloRequest;
    type Response = HelloResponse;

    async fn read_request<T>(&mut self, _: &Self::Protocol, io: &mut T) -> io::Result<Self::Request>
    where
        T: AsyncRead + Unpin + Send,
    {
        let mut buf = Vec::new();
        io.read_to_end(&mut buf).await?;
        Ok(from_slice(&buf).map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?)
    }

    async fn read_response<T>(
        &mut self,
        _: &Self::Protocol,
        io: &mut T,
    ) -> io::Result<Self::Response>
    where
        T: AsyncRead + Unpin + Send,
    {
        let mut buf = Vec::new();
        io.read_to_end(&mut buf).await?;
        Ok(from_slice(&buf).map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?)
    }

    async fn write_request<T>(
        &mut self,
        _: &Self::Protocol,
        io: &mut T,
        req: Self::Request,
    ) -> io::Result<()>
    where
        T: AsyncWrite + Unpin + Send,
    {
        io.write_all(
            &to_vec(&req).map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?,
        )
        .await
    }

    async fn write_response<T>(
        &mut self,
        _: &Self::Protocol,
        io: &mut T,
        res: Self::Response,
    ) -> io::Result<()>
    where
        T: AsyncWrite + Unpin + Send,
    {
        io.write_all(
            &to_vec(&res).map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?,
        )
        .await
    }
}
