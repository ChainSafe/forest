// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use async_trait::async_trait;
use forest_encoding::{from_slice, to_vec};
use futures::prelude::*;
use libp2p::core::ProtocolName;
use libp2p::request_response::RequestResponseCodec;
pub use libp2p::request_response::{RequestId, ResponseChannel};
use serde::{de::DeserializeOwned, Serialize};
use std::io;
use std::marker::PhantomData;

#[derive(Clone)]
pub struct CborRequestResponse<P, RQ, RS> {
    protocol: PhantomData<P>,
    request: PhantomData<RQ>,
    response: PhantomData<RS>,
}

impl<P, RQ, RS> Default for CborRequestResponse<P, RQ, RS> {
    fn default() -> Self {
        Self {
            protocol: PhantomData::<P>::default(),
            request: PhantomData::<RQ>::default(),
            response: PhantomData::<RS>::default(),
        }
    }
}

#[async_trait]
impl<P, RQ, RS> RequestResponseCodec for CborRequestResponse<P, RQ, RS>
where
    P: ProtocolName + Clone + Send + Sync,
    RQ: Serialize + DeserializeOwned + Clone + Send + Sync + 'static,
    RS: Serialize + DeserializeOwned + Clone + Send + Sync + 'static,
{
    type Protocol = P;
    type Request = RQ;
    type Response = RS;

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
        .await?;
        io.close().await?;
        Ok(())
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
        .await?;
        io.close().await?;
        Ok(())
    }
}
