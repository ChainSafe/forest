// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use async_trait::async_trait;
use forest_encoding::to_vec;
use futures::prelude::*;
use futures_cbor_codec::Decoder;
use futures_codec::FramedRead;
use libp2p::core::ProtocolName;
use libp2p::request_response::OutboundFailure;
use libp2p::request_response::RequestResponseCodec;
use serde::{de::DeserializeOwned, Serialize};
use std::io;
use std::marker::PhantomData;

/// Generic Cbor RequestResponse type. This is just needed to satisfy [RequestResponseCodec]
/// for Hello and ChainExchange protocols without duplication.
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

/// libp2p request response outbound error type. This indicates a failure sending a request to
/// a peer. This is different from a failure response from a node, as this is an error that
/// prevented a response.
///
/// This type mirrors the internal libp2p type, but this avoids having to expose that internal type.
#[derive(Debug)]
pub enum RequestResponseError {
    /// The request could not be sent because a dialing attempt failed.
    DialFailure,
    /// The request timed out before a response was received.
    ///
    /// It is not known whether the request may have been
    /// received (and processed) by the remote peer.
    Timeout,
    /// The connection closed before a response was received.
    ///
    /// It is not known whether the request may have been
    /// received (and processed) by the remote peer.
    ConnectionClosed,
    /// The remote supports none of the requested protocols.
    UnsupportedProtocols,
}

impl From<OutboundFailure> for RequestResponseError {
    fn from(err: OutboundFailure) -> Self {
        match err {
            OutboundFailure::DialFailure => Self::DialFailure,
            OutboundFailure::Timeout => Self::Timeout,
            OutboundFailure::ConnectionClosed => Self::ConnectionClosed,
            OutboundFailure::UnsupportedProtocols => Self::UnsupportedProtocols,
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
        let mut reader = FramedRead::new(io, Decoder::<RQ>::new());
        // Expect only one request
        let req = reader
            .next()
            .await
            .transpose()
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?
            .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "read_request returned none"))?;
        Ok(req)
    }

    async fn read_response<T>(
        &mut self,
        _: &Self::Protocol,
        io: &mut T,
    ) -> io::Result<Self::Response>
    where
        T: AsyncRead + Unpin + Send,
    {
        let mut reader = FramedRead::new(io, Decoder::<RS>::new());
        // Expect only one response
        let resp = reader
            .next()
            .await
            .transpose()
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?
            .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "read_response returned none"))?;
        Ok(resp)
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
        // TODO: Use FramedWrite to stream write. Dilemma right now is if we should fork the cbor codec so we can replace serde_cbor to our fork of serde_cbor

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
        // TODO: Use FramedWrite to stream write. Dilemma right now is if we should fork the cbor codec so we can replace serde_cbor to our fork of serde_cbor
        io.write_all(
            &to_vec(&res).map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?,
        )
        .await?;
        io.close().await?;
        Ok(())
    }
}
