// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use async_trait::async_trait;
use futures::prelude::*;
use libp2p::core::ProtocolName;
use libp2p::request_response::OutboundFailure;
use libp2p::request_response::RequestResponseCodec;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::io;
use std::marker::PhantomData;

const MAX_BYTES_ALLOWED: usize = 2 * 1024 * 1024; // messages over 2MB are likely malicious

/// Generic `Cbor` `RequestResponse` type. This is just needed to satisfy [`RequestResponseCodec`]
/// for Hello and `ChainExchange` protocols without duplication.
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

/// Libp2p request response outbound error type. This indicates a failure sending a request to
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
    RQ: Serialize + DeserializeOwned + Send + Sync + 'static,
    RS: Serialize + DeserializeOwned + Send + Sync + 'static,
{
    type Protocol = P;
    type Request = RQ;
    type Response = RS;

    async fn read_request<T>(&mut self, _: &Self::Protocol, io: &mut T) -> io::Result<Self::Request>
    where
        T: AsyncRead + Unpin + Send,
    {
        let bytes = read_with_limit(io, MAX_BYTES_ALLOWED).await?;
        serde_ipld_dagcbor::de::from_reader(bytes.as_slice()).map_err(|e| {
            io::Error::new(io::ErrorKind::Other, format!("dagcbor decoding error: {e}"))
        })
    }

    async fn read_response<T>(
        &mut self,
        _: &Self::Protocol,
        io: &mut T,
    ) -> io::Result<Self::Response>
    where
        T: AsyncRead + Unpin + Send,
    {
        let bytes = read_with_limit(io, MAX_BYTES_ALLOWED).await?;
        serde_ipld_dagcbor::de::from_reader(bytes.as_slice()).map_err(|e| {
            io::Error::new(io::ErrorKind::Other, format!("dagcbor decoding error: {e}"))
        })
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
            &fvm_ipld_encoding::to_vec(&req)
                .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?,
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
            &fvm_ipld_encoding::to_vec(&res)
                .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?,
        )
        .await?;
        io.close().await?;
        Ok(())
    }
}

async fn read_with_limit<T>(io: &mut T, max_bytes: usize) -> io::Result<Vec<u8>>
where
    T: AsyncRead + Unpin,
{
    let mut v = Vec::with_capacity(max_bytes);
    let mut bytes_read = 0;
    let mut buffer = [0; 1024];
    'l: loop {
        let size = io.read(&mut buffer).await?;
        if size == 0 {
            break 'l;
        }
        bytes_read += size;
        if bytes_read > max_bytes {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                format!("Buffer size exceeds the maximum allowed {max_bytes}B"),
            ));
        }
        v.extend_from_slice(&buffer[..size]);
    }

    Ok(v)
}
