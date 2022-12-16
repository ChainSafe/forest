// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod decoder;
use decoder::DagCborDecodingReader;

use async_trait::async_trait;
use futures::prelude::*;
use libp2p::core::ProtocolName;
use libp2p::request_response::OutboundFailure;
use libp2p::request_response::RequestResponseCodec;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::io;
use std::marker::PhantomData;
use std::time::Duration;

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
        read_and_decode(io).await
    }

    async fn read_response<T>(
        &mut self,
        _: &Self::Protocol,
        io: &mut T,
    ) -> io::Result<Self::Response>
    where
        T: AsyncRead + Unpin + Send,
    {
        read_and_decode(io).await
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

async fn read_and_decode<IO, T>(io: &mut IO) -> io::Result<T>
where
    IO: AsyncRead + Unpin,
    T: serde::de::DeserializeOwned,
{
    // FIXME: investigate the best value here, 2MB is not enough for mainnet epoch 2419928
    // Issue: https://github.com/ChainSafe/forest/issues/2362
    const MAX_BYTES_ALLOWED: usize = 200 * 1024 * 1024; // messages over 200MB are likely malicious
    const TIMEOUT: Duration = Duration::from_secs(30);

    // Currently the protocol does not send length encoded message,
    // and we use `decode-success-with-no-trailing-data` to detect end of frame
    // just like what `FramedRead` does, so it's possible to cause deadlock at `io.poll_ready`
    // Adding timeout here to mitigate the issue
    match tokio::time::timeout(TIMEOUT, DagCborDecodingReader::new(io, MAX_BYTES_ALLOWED)).await {
        Ok(r) => r,
        Err(_) => {
            let err = io::Error::new(io::ErrorKind::Other, "read_and_decode timeout");
            log::warn!("{err}");
            Err(err)
        }
    }
}

async fn encode_and_write<IO, T>(io: &mut IO, data: T) -> io::Result<()>
where
    IO: AsyncWrite + Unpin,
    T: serde::Serialize,
{
    let bytes = fvm_ipld_encoding::to_vec(&data)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;
    io.write_all(&bytes).await?;
    io.close().await?;
    Ok(())
}
