// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{io, marker::PhantomData, pin::Pin, task::Poll};

use bytes::BytesMut;
use futures::prelude::*;
use pin_project_lite::pin_project;
use tracing::warn;

#[derive(Debug, Clone, Copy)]
pub(super) enum DecodeMode {
    /// Decode once the stream reaches `EOF`. The peer half-closes after writing, so the whole value
    /// is buffered by then and one decode is enough.
    AtEof,
    /// Decode after every read, for a peer that keeps the stream open after writing so `EOF` only
    /// arrives after we reply (the request path).
    PerRead,
}

pin_project! {
    #[derive(Debug)]
    pub(super) struct DagCborDecodingReader<B, T> {
        #[pin]
        io: B,
        max_bytes_allowed: usize,
        mode: DecodeMode,
        bytes: BytesMut,
        bytes_read: usize,
        _pd: PhantomData<T>,
    }
}

impl<B, T> DagCborDecodingReader<B, T> {
    /// `max_bytes_allowed == 0` means unlimited
    pub(super) fn new(io: B, max_bytes_allowed: usize, mode: DecodeMode) -> Self {
        Self {
            io,
            max_bytes_allowed,
            mode,
            bytes: BytesMut::new(),
            bytes_read: 0,
            _pd: Default::default(),
        }
    }
}

impl<B, T> Future for DagCborDecodingReader<B, T>
where
    B: AsyncRead,
    T: serde::de::DeserializeOwned,
{
    type Output = io::Result<T>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> Poll<Self::Output> {
        // https://github.com/mxinden/asynchronous-codec/blob/master/src/framed_read.rs#L161
        let mut buf = [0u8; 8 * 1024];
        loop {
            let this = self.as_mut().project();
            let n = std::task::ready!(this.io.poll_read(cx, &mut buf))?;
            // Terminated
            if n == 0 {
                let item =
                    serde_ipld_dagcbor::de::from_reader(&self.bytes[..]).map_err(io::Error::other);
                return Poll::Ready(item);
            }
            *this.bytes_read += n;
            if *this.max_bytes_allowed > 0 && *this.bytes_read > *this.max_bytes_allowed {
                let err = io::Error::other(format!(
                    "Buffer size exceeds the maximum allowed {}B",
                    *this.max_bytes_allowed,
                ));
                warn!("{err}");
                return Poll::Ready(Err(err));
            }
            #[allow(clippy::indexing_slicing)]
            this.bytes.extend_from_slice(&buf[..n.min(buf.len())]);
            // Note: `from_reader` ensures no trailing data left in `bytes`
            if let DecodeMode::PerRead = this.mode
                && let Ok(r) = serde_ipld_dagcbor::de::from_reader(&this.bytes[..])
            {
                return Poll::Ready(Ok(r));
            }
        }
    }
}

#[cfg(test)]
mod decoder_tests {
    use super::*;

    const MIB: usize = 1024 * 1024;

    fn decode<T: serde::de::DeserializeOwned>(bytes: Vec<u8>, mode: DecodeMode) -> io::Result<T> {
        let reader = futures::io::Cursor::new(bytes);
        futures::executor::block_on(DagCborDecodingReader::new(reader, 120 * MIB, mode))
    }

    #[test]
    fn both_modes_decode_a_complete_value() {
        let value = ipld_core::ipld::Ipld::List(vec![
            ipld_core::ipld::Ipld::Integer(1),
            ipld_core::ipld::Ipld::Bytes(vec![0u8; 64 * 1024]),
        ]);
        let bytes = serde_ipld_dagcbor::to_vec(&value).unwrap();
        for mode in [DecodeMode::AtEof, DecodeMode::PerRead] {
            let decoded: ipld_core::ipld::Ipld = decode(bytes.clone(), mode).unwrap();
            assert_eq!(decoded, value, "{mode:?} must decode a complete value");
        }
    }

    /// Delivers `data` one byte per read, then `Pending` forever: a peer that wrote a complete value
    /// but keeps the stream open (the request path).
    struct DataThenPending {
        data: Vec<u8>,
        pos: usize,
    }

    impl AsyncRead for DataThenPending {
        fn poll_read(
            mut self: Pin<&mut Self>,
            _cx: &mut std::task::Context<'_>,
            buf: &mut [u8],
        ) -> Poll<io::Result<usize>> {
            let (Some(&byte), Some(slot)) = (self.data.get(self.pos), buf.first_mut()) else {
                return Poll::Pending;
            };
            *slot = byte;
            self.pos += 1;
            Poll::Ready(Ok(1))
        }
    }

    #[test]
    fn per_read_completes_before_eof() {
        // The request path sees no EOF before we reply, so PerRead must return once the buffer holds
        // a complete value.
        let value = ipld_core::ipld::Ipld::List(vec![ipld_core::ipld::Ipld::Integer(7); 4]);
        let bytes = serde_ipld_dagcbor::to_vec(&value).unwrap();
        let reader = DataThenPending {
            data: bytes,
            pos: 0,
        };
        let decoded: ipld_core::ipld::Ipld = futures::executor::block_on(
            DagCborDecodingReader::new(reader, 120 * MIB, DecodeMode::PerRead),
        )
        .unwrap();
        assert_eq!(decoded, value);
    }

    #[test]
    fn at_eof_does_not_decode_before_eof() {
        // AtEof decodes only at the half-close: a complete value whose stream is still open stays
        // pending instead of resolving early.
        let value = ipld_core::ipld::Ipld::List(vec![ipld_core::ipld::Ipld::Integer(7); 4]);
        let bytes = serde_ipld_dagcbor::to_vec(&value).unwrap();
        let fut = DagCborDecodingReader::<_, ipld_core::ipld::Ipld>::new(
            DataThenPending {
                data: bytes,
                pos: 0,
            },
            120 * MIB,
            DecodeMode::AtEof,
        );
        futures::pin_mut!(fut);
        let mut cx = std::task::Context::from_waker(futures::task::noop_waker_ref());
        assert!(matches!(fut.as_mut().poll(&mut cx), Poll::Pending));
    }
}
