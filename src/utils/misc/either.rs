// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{
    pin::Pin,
    task::{Context, Poll},
};

use futures::{AsyncRead, AsyncWrite};

pub enum Either<L, R> {
    Left(L),
    Right(R),
}

impl<L: AsyncRead + Unpin, R: AsyncRead + Unpin> AsyncRead for Either<L, R> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<std::io::Result<usize>> {
        match Pin::into_inner(self) {
            Self::Left(left) => Pin::new(left).poll_read(cx, buf),
            Self::Right(right) => Pin::new(right).poll_read(cx, buf),
        }
    }
}

impl<L: AsyncWrite + Unpin, R: AsyncWrite + Unpin> AsyncWrite for Either<L, R> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        match Pin::into_inner(self) {
            Self::Left(left) => Pin::new(left).poll_write(cx, buf),
            Self::Right(right) => Pin::new(right).poll_write(cx, buf),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        match Pin::into_inner(self) {
            Self::Left(left) => Pin::new(left).poll_flush(cx),
            Self::Right(right) => Pin::new(right).poll_flush(cx),
        }
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        match Pin::into_inner(self) {
            Self::Left(left) => Pin::new(left).poll_close(cx),
            Self::Right(right) => Pin::new(right).poll_close(cx),
        }
    }
}
