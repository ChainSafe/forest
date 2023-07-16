// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::io;

use bytes::{BufMut as _, BytesMut};
use cid::Cid;
use futures::stream::{self, StreamExt as _, TryStream, TryStreamExt as _};
use fvm_ipld_car::CarHeader;
use std::future;
// use libipld_macro::Ipld;
use crate::{
    car_backed_blockstore::{
        // TODO(aatifsyed): these shouldn't need to be public
        varint_to_zstd_frame_collator,
        zstd_compress_finish,
    },
    utils::try_collate,
};
use tokio::io::AsyncWrite;
use tokio_util::codec::{BytesCodec, FramedWrite};
use tokio_util_06::codec::FramedWrite as FramedWrite06;

type VarintFrameCodec = unsigned_varint::codec::UviBytes<BytesMut>;

#[allow(clippy::enum_variant_names)] // V2 support soon
pub enum CarFormat {
    V1Plain,
    /// See [crate::car_backed_blockstore::CompressedCarV1BackedBlockstore]
    V1ManyFrame {
        zstd_frame_size_tripwire: usize,
        zstd_compression_level: u16,
    },
    V1ManyFrameIndexedOutOfBand {
        zstd_frame_size_tripwire: usize,
        zstd_compression_level: u16,
        write_to: Box<dyn AsyncWrite>,
    },
}

pub async fn write_car(
    format: CarFormat,
    roots: Vec<Cid>,
    // TODO(aatifsyed): can we be smarter about the serialization here?
    // TODO(aatifsyed): should this accept (Cid, Ipld)?
    blocks: impl TryStream<Ok = (Cid, Vec<u8>), Error = io::Error>,
    // TODO(aatifsyed): document that this should be uncompressed for manyframe formats
    writer: impl AsyncWrite,
) -> io::Result<()> {
    match format {
        CarFormat::V1Plain => {
            stream::once(future::ready(Ok(v1_header(roots))))
                .chain(blocks.map_ok(|(cid, ipld)| cid_and_ipld(cid, ipld)))
                .forward(FramedWrite06::new(writer, VarintFrameCodec::default()))
                .await
        }
        CarFormat::V1ManyFrame {
            zstd_frame_size_tripwire,
            zstd_compression_level,
        } => {
            try_collate(
                stream::once(future::ready(Ok(v1_header(roots))))
                    .chain(blocks.map_ok(|(cid, ipld)| cid_and_ipld(cid, ipld))),
                varint_to_zstd_frame_collator(zstd_frame_size_tripwire, zstd_compression_level),
                zstd_compress_finish,
            )
            .forward(FramedWrite::new(writer, BytesCodec::default()))
            .await
        }
        CarFormat::V1ManyFrameIndexedOutOfBand {
            zstd_frame_size_tripwire,
            zstd_compression_level,
            write_to,
        } => {
            todo!("how should we index as we stream?")
        }
    }
}

fn v1_header(roots: Vec<Cid>) -> BytesMut {
    let mut buffer = BytesMut::new();
    let header = CarHeader { roots, version: 1 };
    fvm_ipld_encoding::to_writer((&mut buffer).writer(), &header).expect(
        "BytesMut has infallible IO, and CarHeader probably doesn't validate on serialization",
    );
    buffer
}

// TODO(aatifsyed): don't actually need to take Vec<u8>..
fn cid_and_ipld(cid: Cid, ipld: Vec<u8>) -> BytesMut {
    let mut buffer = BytesMut::new();
    cid.write_bytes((&mut buffer).writer())
        .expect("BytesMut has infallible IO");
    buffer.extend(ipld);
    buffer
}
