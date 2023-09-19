// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use async_compression::tokio::bufread::ZstdDecoder;
use bytes::{Buf, BufMut, Bytes, BytesMut};
use cid::{
    multihash::{Code, MultihashDigest},
    Cid,
};
use futures::ready;
use futures::{sink::Sink, Stream, StreamExt};
use fvm_ipld_encoding::to_vec;
use integer_encoding::VarInt;
use pin_project_lite::pin_project;
use serde::{Deserialize, Serialize};
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncRead, AsyncWrite};
use tokio_util::codec::Encoder;
use tokio_util::codec::FramedRead;
use tokio_util::either::Either;
use unsigned_varint::codec::UviBytes;

use crate::utils::encoding::from_slice_with_fallback;

#[derive(Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct CarHeader {
    pub roots: Vec<Cid>,
    pub version: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CarBlock {
    pub cid: Cid,
    pub data: Vec<u8>,
}

impl CarBlock {
    // Write a varint frame containing the cid and the data
    pub fn write(&self, mut writer: &mut impl io::Write) -> io::Result<()> {
        let frame_length = self.cid.encoded_len() + self.data.len();
        writer.write_all(&frame_length.encode_var_vec())?;
        self.cid
            .write_bytes(&mut writer)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        writer.write_all(&self.data)?;
        Ok(())
    }

    pub fn from_bytes(bytes: impl Into<Bytes>) -> io::Result<CarBlock> {
        let bytes: Bytes = bytes.into();
        let mut cursor = bytes.reader();
        let cid = Cid::read_bytes(&mut cursor)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        let bytes = cursor.into_inner();
        Ok(CarBlock {
            cid,
            data: bytes.to_vec(),
        })
    }

    pub fn valid(&self) -> bool {
        if let Ok(code) = Code::try_from(self.cid.hash().code()) {
            let actual = Cid::new_v1(self.cid.codec(), code.digest(&self.data));
            actual == self.cid
        } else {
            false
        }
    }
}

pin_project! {
    /// Stream of CAR blocks. If the input data is compressed with zstd, it will
    /// automatically be decompressed.
    pub struct CarStream<ReaderT> {
        #[pin]
        reader: FramedRead<Either<ReaderT, ZstdDecoder<ReaderT>>, UviBytes>,
        pub header: CarHeader,
        first_block: Option<CarBlock>,
    }
}

// This method checks the header in order to see whether or not we are operating on a zstd
// archive. The zstd header has a maximum size of 18 bytes:
// https://github.com/facebook/zstd/blob/dev/doc/zstd_compression_format.md#zstandard-frames.
fn is_zstd(buf: &[u8]) -> bool {
    zstd::zstd_safe::get_frame_content_size(buf).is_ok()
}

impl<ReaderT: AsyncBufRead + Unpin> CarStream<ReaderT> {
    pub async fn new(mut reader: ReaderT) -> io::Result<Self> {
        let is_compressed = is_zstd(reader.fill_buf().await?);
        let mut reader = if is_compressed {
            let mut zstd = ZstdDecoder::new(reader);
            zstd.multiple_members(true);
            FramedRead::new(Either::Right(zstd), UviBytes::default())
        } else {
            FramedRead::new(Either::Left(reader), UviBytes::default())
        };
        let header = read_header(&mut reader).await.ok_or(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid header block",
        ))?;

        // Read the first block and check if it is valid. This check helps to
        // catch invalid CAR files as soon as we open.
        if let Some(first_entry) = reader.next().await.transpose()? {
            let block = CarBlock::from_bytes(first_entry)?;
            if !block.valid() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "invalid first block",
                ));
            }
            Ok(CarStream {
                reader,
                header,
                first_block: Some(block),
            })
        } else {
            Ok(CarStream {
                reader,
                header,
                first_block: None,
            })
        }
    }
}

impl<ReaderT: AsyncBufRead> Stream for CarStream<ReaderT> {
    type Item = io::Result<CarBlock>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.project();
        if let Some(block) = this.first_block.take() {
            return Poll::Ready(Some(Ok(block)));
        }
        let item = futures::ready!(this.reader.poll_next(cx));
        Poll::Ready(item.map(|ret| ret.and_then(CarBlock::from_bytes)))
    }
}

pin_project! {
    pub struct CarWriter<W> {
        #[pin]
        inner: W,
        buffer: BytesMut,
    }
}

impl<W: AsyncWrite> CarWriter<W> {
    pub fn new_carv1(roots: Vec<Cid>, writer: W) -> anyhow::Result<Self> {
        let car_header = CarHeader { roots, version: 1 };

        let mut header_uvi_frame = BytesMut::new();
        UviBytes::default().encode(Bytes::from(to_vec(&car_header)?), &mut header_uvi_frame)?;

        Ok(Self {
            inner: writer,
            buffer: header_uvi_frame,
        })
    }
}

impl<W: AsyncWrite> Sink<CarBlock> for CarWriter<W> {
    type Error = io::Error;

    fn poll_ready(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let mut this = self.as_mut().project();

        while !this.buffer.is_empty() {
            this = self.as_mut().project();
            let bytes_written = ready!(this.inner.poll_write(cx, this.buffer))?;
            this.buffer.advance(bytes_written);
        }
        Poll::Ready(Ok(()))
    }
    fn start_send(self: Pin<&mut Self>, item: CarBlock) -> Result<(), Self::Error> {
        item.write(&mut self.project().buffer.writer())
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        ready!(self.as_mut().poll_ready(cx))?;
        self.project().inner.poll_flush(cx)
    }
    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        ready!(self.as_mut().poll_ready(cx))?;
        self.project().inner.poll_shutdown(cx)
    }
}

async fn read_header<ReaderT: AsyncRead + Unpin>(
    framed_reader: &mut FramedRead<ReaderT, UviBytes>,
) -> Option<CarHeader> {
    let header = from_slice_with_fallback::<CarHeader>(&framed_reader.next().await?.ok()?).ok()?;
    if header.version != 1 {
        return None;
    }
    Some(header)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::networks::{calibnet, mainnet};
    use futures::TryStreamExt;
    use quickcheck::{Arbitrary, Gen};

    impl Arbitrary for CarBlock {
        fn arbitrary(g: &mut Gen) -> CarBlock {
            let data = Vec::<u8>::arbitrary(g);
            let encoding = g
                .choose(&[
                    fvm_ipld_encoding::DAG_CBOR,
                    fvm_ipld_encoding::CBOR,
                    fvm_ipld_encoding::IPLD_RAW,
                ])
                .unwrap();
            let code = g.choose(&[Code::Blake2b256, Code::Sha2_256]).unwrap();
            let cid = Cid::new_v1(*encoding, code.digest(&data));
            CarBlock { cid, data }
        }
    }

    #[tokio::test]
    async fn stream_calibnet_genesis() {
        let stream = CarStream::new(calibnet::DEFAULT_GENESIS).await.unwrap();
        let blocks: Vec<CarBlock> = stream.try_collect().await.unwrap();
        assert_eq!(blocks.len(), 1207);
        for block in blocks {
            assert!(block.valid());
        }
    }

    #[tokio::test]
    async fn stream_mainnet_genesis() {
        let stream = CarStream::new(mainnet::DEFAULT_GENESIS).await.unwrap();
        let blocks: Vec<CarBlock> = stream.try_collect().await.unwrap();
        assert_eq!(blocks.len(), 1222);
        for block in blocks {
            assert!(block.valid());
        }
    }
}
