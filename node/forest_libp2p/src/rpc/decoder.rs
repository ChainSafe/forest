// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use bytes::BytesMut;
use futures::prelude::*;
use pin_project_lite::pin_project;
use std::io;
use std::marker::PhantomData;
use std::pin::Pin;
use std::task::Poll;

pin_project! {
    #[derive(Debug)]
    pub(super) struct DagCborDecodingReader<B, T> {
        #[pin]
        io: B,
        max_bytes_allowed: usize,
        bytes: BytesMut,
        bytes_read: usize,
        _pd: PhantomData<T>,
    }
}

impl<B, T> DagCborDecodingReader<B, T> {
    /// `max_bytes_allowed == 0` means unlimited
    pub(super) fn new(io: B, max_bytes_allowed: usize) -> Self {
        Self {
            io,
            max_bytes_allowed,
            bytes: BytesMut::new(),
            bytes_read: 0,
            _pd: Default::default(),
        }
    }
}

impl<B, T> Future for DagCborDecodingReader<B, T>
where
    B: AsyncRead + Unpin,
    T: serde::de::DeserializeOwned,
{
    type Output = io::Result<T>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> Poll<Self::Output> {
        let mut buf = [0u8; 1024];
        loop {
            let n = std::task::ready!(Pin::new(&mut self.io).poll_read(cx, &mut buf))?;
            // Terminated
            if n == 0 {
                let item = serde_ipld_dagcbor::de::from_reader(&self.bytes[..])
                    .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()));
                return Poll::Ready(item);
            }
            self.bytes_read += n;
            if self.max_bytes_allowed > 0 && self.bytes_read > self.max_bytes_allowed {
                return Poll::Ready(Err(io::Error::new(
                    io::ErrorKind::Other,
                    format!(
                        "Buffer size exceeds the maximum allowed {}B",
                        self.max_bytes_allowed,
                    ),
                )));
            }
            self.bytes.extend_from_slice(&buf[..n.min(buf.len())]);
            // This is what `FramedRead` does internally
            // Assuming io will be re-used to send new messages.
            //
            // Note: `from_reader` ensures no trailing data left in `bytes`
            if let Ok(r) = serde_ipld_dagcbor::de::from_reader(&self.bytes[..]) {
                return Poll::Ready(Ok(r));
            }
        }
    }
}
