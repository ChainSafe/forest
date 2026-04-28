// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod decoder;
use std::{io, marker::PhantomData, time::Duration};

use async_trait::async_trait;
use decoder::DagCborDecodingReader;
use futures::prelude::*;
use libp2p::request_response::{self, OutboundFailure};
use serde::{Serialize, de::DeserializeOwned};

/// Generic `Cbor` `RequestResponse` type. This is just needed to satisfy
/// [`request_response::Codec`] for Hello and `ChainExchange` protocols without
/// duplication.
///
/// Pick a tight `MAX_RESPONSE_BYTES` for fixed-shape protocols (Hello) and a
/// generous one for bulk transfers (`ChainExchange`).
#[derive(Clone)]
pub struct CborRequestResponse<P, RQ, RS, const MAX_RESPONSE_BYTES: usize> {
    protocol: PhantomData<P>,
    request: PhantomData<RQ>,
    response: PhantomData<RS>,
}

impl<P, RQ, RS, const MAX_RESPONSE_BYTES: usize> Default
    for CborRequestResponse<P, RQ, RS, MAX_RESPONSE_BYTES>
{
    fn default() -> Self {
        Self {
            protocol: PhantomData::<P>,
            request: PhantomData::<RQ>,
            response: PhantomData::<RS>,
        }
    }
}

/// Libp2p request response outbound error type. This indicates a failure
/// sending a request to a peer. This is different from a failure response from
/// a node, as this is an error that prevented a response.
///
/// This type mirrors the internal libp2p type, but this avoids having to expose
/// that internal type.
#[derive(Debug, thiserror::Error)]
pub enum RequestResponseError {
    /// The request could not be sent because a dialing attempt failed.
    #[error("DialFailure")]
    DialFailure,
    /// The request timed out before a response was received.
    ///
    /// It is not known whether the request may have been
    /// received (and processed) by the remote peer.
    #[error("Timeout")]
    Timeout,
    /// The connection closed before a response was received.
    ///
    /// It is not known whether the request may have been
    /// received (and processed) by the remote peer.
    #[error("ConnectionClosed")]
    ConnectionClosed,
    /// The remote supports none of the requested protocols.
    #[error("UnsupportedProtocols")]
    UnsupportedProtocols,
    /// An IO failure happened on an outbound stream.
    #[error("{0}")]
    Io(io::Error),
}

impl From<OutboundFailure> for RequestResponseError {
    fn from(err: OutboundFailure) -> Self {
        match err {
            OutboundFailure::DialFailure => Self::DialFailure,
            OutboundFailure::Timeout => Self::Timeout,
            OutboundFailure::ConnectionClosed => Self::ConnectionClosed,
            OutboundFailure::UnsupportedProtocols => Self::UnsupportedProtocols,
            OutboundFailure::Io(e) => Self::Io(e),
        }
    }
}

#[async_trait]
impl<P, RQ, RS, const MAX_RESPONSE_BYTES: usize> request_response::Codec
    for CborRequestResponse<P, RQ, RS, MAX_RESPONSE_BYTES>
where
    P: AsRef<str> + Send + Clone,
    RQ: Serialize + DeserializeOwned + Send + Sync,
    RS: Serialize + DeserializeOwned + Send + Sync,
{
    type Protocol = P;
    type Request = RQ;
    type Response = RS;

    async fn read_request<T>(&mut self, _: &Self::Protocol, io: &mut T) -> io::Result<Self::Request>
    where
        T: AsyncRead + Unpin + Send,
    {
        read_request_and_decode(io).await
    }

    async fn read_response<T>(
        &mut self,
        _: &Self::Protocol,
        io: &mut T,
    ) -> io::Result<Self::Response>
    where
        T: AsyncRead + Unpin + Send,
    {
        timed_decode(io, MAX_RESPONSE_BYTES).await
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
        encode_and_write(io, req).await
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
        encode_and_write(io, res).await
    }
}

// Because of how lotus implements the protocol, it will deadlock when calling
// `io.ReadToEnd` on requests.
//
// for sending requests, the flow in lotus is
// 1. write encoded request bytes
// 2. wait for response
// 3. close request stream, which sends `FIN` header over `yamux` protocol
// if we call `io.ReadToEnd` before `FIN` is sent, it will deadlock
//
// but for sending responses, the flow in lotus is
// 1. receive request
// 2. write encode response bytes
// 3. close response stream, which sends `FIN` header over `yamux` protocol
// and we call `io.ReadToEnd` after `FIN` is sent, it will not deadlock
//
// Note: `FIN` - Performs a half-close of a stream. May be sent with a data
// message or window update. See <https://github.com/libp2p/go-yamux/blob/master/spec.md#flag-field>
//
// `io` is essentially [yamux::Stream](https://docs.rs/yamux/0.11.0/yamux/struct.Stream.html)
//
async fn read_request_and_decode<IO, T>(io: &mut IO) -> io::Result<T>
where
    IO: AsyncRead + Unpin,
    T: serde::de::DeserializeOwned,
{
    // Requests over 2 MiB are likely malicious for both protocols we support.
    const MAX_BYTES_ALLOWED: usize = 2 * 1024 * 1024;
    timed_decode(io, MAX_BYTES_ALLOWED).await
}

/// Decodes a response from the given `io` with a timeout. This is used to prevent hanging when a peer fails to respond in a timely manner.
async fn timed_decode<IO, T>(io: &mut IO, max_bytes: usize) -> io::Result<T>
where
    IO: AsyncRead + Unpin,
    T: serde::de::DeserializeOwned,
{
    const DECODE_TIMEOUT: Duration = Duration::from_secs(30);
    match tokio::time::timeout(DECODE_TIMEOUT, DagCborDecodingReader::new(io, max_bytes)).await {
        Ok(r) => r,
        Err(_) => {
            let err = io::Error::from(io::ErrorKind::TimedOut);
            tracing::debug!("{err}");
            Err(err)
        }
    }
}

async fn encode_and_write<IO, T>(io: &mut IO, data: T) -> io::Result<()>
where
    IO: AsyncWrite + Unpin,
    T: serde::Serialize,
{
    let bytes = fvm_ipld_encoding::to_vec(&data).map_err(io::Error::other)?;
    io.write_all(&bytes).await?;
    io.close().await?;
    Ok(())
}
