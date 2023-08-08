// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use async_compression::tokio::bufread::ZstdDecoder;
use bytes::{Buf, Bytes};
use cid::{
    multihash::{Code, MultihashDigest},
    Cid,
};
use futures::{Stream, StreamExt};
use integer_encoding::VarInt;
use pin_project_lite::pin_project;
use serde::{Deserialize, Serialize};
use std::io::{self, Cursor, SeekFrom};
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncBufRead, AsyncRead, AsyncSeek, AsyncSeekExt};
use tokio_util::codec::FramedRead;
use tokio_util::either::Either;

use crate::utils::encoding::{from_slice_with_fallback, uvibytes::UviBytes};

#[derive(Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct CarHeader {
    pub roots: Vec<Cid>,
    pub version: u64,
}

#[derive(Debug, Clone)]
pub struct Block {
    pub cid: Cid,
    pub data: Vec<u8>,
}

impl Block {
    // Write a varint frame containing the cid and the data
    pub fn write(&self, mut writer: &mut impl std::io::Write) -> io::Result<()> {
        let frame_length = self.cid.encoded_len() + self.data.len();
        writer.write_all(&frame_length.encode_var_vec())?;
        self.cid
            .write_bytes(&mut writer)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        writer.write_all(&self.data)?;
        Ok(())
    }

    pub fn from_bytes(bytes: Bytes) -> Option<Block> {
        let mut cursor = Cursor::new(bytes);
        let cid = Cid::read_bytes(&mut cursor).ok()?;
        let data_offset = cursor.position();
        let mut bytes = cursor.into_inner();
        bytes.advance(data_offset as usize);
        Some(Block {
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
    }
}

impl<ReaderT: AsyncSeek + AsyncBufRead + Unpin> CarStream<ReaderT> {
    pub async fn new(mut reader: ReaderT) -> io::Result<Self> {
        let start_position = reader.stream_position().await?;
        if let Some(header) = read_header(&mut reader).await {
            reader.seek(SeekFrom::Start(start_position)).await?;
            let mut framed_reader = FramedRead::new(Either::Left(reader), UviBytes::default());
            let _ = framed_reader.next().await;
            Ok(CarStream {
                reader: framed_reader,
                header,
            })
        } else {
            reader.seek(SeekFrom::Start(start_position)).await?;
            let mut zstd = ZstdDecoder::new(reader);
            zstd.multiple_members(true);
            if let Some(header) = read_header(&mut zstd).await {
                let mut reader = zstd.into_inner();

                reset_bufread(&mut reader).await?;

                reader.seek(SeekFrom::Start(start_position)).await?;
                let mut zstd = ZstdDecoder::new(reader);
                zstd.multiple_members(true);
                let mut framed_reader = FramedRead::new(Either::Right(zstd), UviBytes::default());
                let _ = framed_reader.next().await;
                Ok(CarStream {
                    reader: framed_reader,
                    header,
                })
            } else {
                Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "CAR data not recognized",
                ))
            }
        }
    }
}

impl<ReaderT: AsyncBufRead> Stream for CarStream<ReaderT> {
    type Item = io::Result<Block>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.project();
        let item = futures::ready!(this.reader.poll_next(cx));
        Poll::Ready(item.map(|ret| {
            ret.and_then(|bytes| {
                let mut cursor = Cursor::new(bytes);
                let cid = Cid::read_bytes(&mut cursor)
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
                let data_offset = cursor.position();
                let mut bytes = cursor.into_inner();
                bytes.advance(data_offset as usize);
                Ok(Block {
                    cid,
                    data: bytes.to_vec(),
                })
            })
        }))
    }
}

async fn read_header<ReaderT: AsyncRead + Unpin>(reader: &mut ReaderT) -> Option<CarHeader> {
    let mut framed_reader = FramedRead::new(reader, UviBytes::default());
    let header = from_slice_with_fallback::<CarHeader>(&framed_reader.next().await?.ok()?).ok()?;
    if header.version != 1 {
        return None;
    }
    let first_block = Block::from_bytes(framed_reader.next().await?.ok()?)?;
    if !first_block.valid() {
        return None;
    }

    Some(header)
}

// Seeking fails after we've used the Reader for zstd decoding. Flushing the
// buffer "fixes" the problem.
async fn reset_bufread<ReaderT: AsyncBufRead + Unpin>(mut reader: &mut ReaderT) -> io::Result<()> {
    let size = futures::future::poll_fn(|cx| {
        let buf = futures::ready!(Pin::new(&mut reader).poll_fill_buf(cx))?;
        Poll::Ready(Ok::<usize, io::Error>(buf.len()))
    })
    .await?;
    Pin::new(&mut reader).consume(size);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use quickcheck::{Arbitrary, Gen};
    // use quickcheck_macros::quickcheck;

    impl Arbitrary for Block {
        fn arbitrary(g: &mut Gen) -> Block {
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
            Block { cid, data }
        }
    }
}
