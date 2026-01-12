// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::{
    blocks::*,
    db::MemoryDB,
    networks::{calibnet, mainnet},
    utils::rand::forest_rng,
};
use futures::TryStreamExt;
use quickcheck::{Arbitrary, Gen};
use rand::Rng as _;
use sha2::Sha256;
use std::{fs::File, io::Cursor, sync::Arc};

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
        let code = g
            .choose(&[MultihashCode::Blake2b256, MultihashCode::Sha2_256])
            .unwrap();
        let cid = Cid::new_v1(*encoding, code.digest(&data));
        CarBlock { cid, data }
    }
}

#[tokio::test]
async fn stream_calibnet_genesis_unsafe() {
    let stream = CarStream::new_unsafe(calibnet::DEFAULT_GENESIS)
        .await
        .unwrap();
    let blocks: Vec<CarBlock> = stream.try_collect().await.unwrap();
    assert_eq!(blocks.len(), 1207);
    for block in blocks {
        block.validate().unwrap();
    }
}

#[tokio::test]
async fn stream_calibnet_genesis() {
    let stream = CarStream::new(Cursor::new(calibnet::DEFAULT_GENESIS))
        .await
        .unwrap();
    let blocks: Vec<CarBlock> = stream.try_collect().await.unwrap();
    assert_eq!(blocks.len(), 1207);
    for block in blocks {
        block.validate().unwrap();
    }
}

#[tokio::test]
async fn stream_mainnet_genesis_unsafe() {
    let stream = CarStream::new_unsafe(mainnet::DEFAULT_GENESIS)
        .await
        .unwrap();
    let blocks: Vec<CarBlock> = stream.try_collect().await.unwrap();
    assert_eq!(blocks.len(), 1222);
    for block in blocks {
        block.validate().unwrap();
    }
}

#[tokio::test]
async fn stream_mainnet_genesis() {
    let stream = CarStream::new(Cursor::new(mainnet::DEFAULT_GENESIS))
        .await
        .unwrap();
    let blocks: Vec<CarBlock> = stream.try_collect().await.unwrap();
    assert_eq!(blocks.len(), 1222);
    for block in blocks {
        block.validate().unwrap();
    }
}

#[tokio::test]
async fn stream_snapshot_parity() {
    let db = Arc::new(MemoryDB::default());
    let c4u = Chain4U::with_blockstore(db.clone());
    chain4u! {
        in c4u; // select the context
        [_genesis]
        -> [_b_1]
        -> [_b_2_0, _b_2_1]
        -> [_b_3]
        -> [_b_4]
        -> [b_5_0, b_5_1]
    };

    let head_key_cids = nunny::vec![b_5_0.cid(), b_5_1.cid()];
    let head_key = TipsetKey::from(head_key_cids.clone());
    let head = Tipset::load_required(&db, &head_key).unwrap();

    let stream_v1 = {
        let mut snap_bytes: Vec<u8> = vec![];
        crate::chain::export::<Sha256>(&db, &head, 0, &mut snap_bytes, None)
            .await
            .unwrap();
        CarStream::new_with_header_v2(Cursor::new(snap_bytes), None)
            .await
            .unwrap()
    };
    let blocks_v1: Vec<CarBlock> = stream_v1.try_collect().await.unwrap();
    for block in &blocks_v1 {
        block.validate().unwrap();
    }

    let stream_v2 = {
        let mut snap_bytes: Vec<u8> = vec![];
        crate::chain::export_v2::<Sha256, File>(&db, None, &head, 0, &mut snap_bytes, None)
            .await
            .unwrap();
        CarStream::new_with_header_v2(Cursor::new(snap_bytes), None)
            .await
            .unwrap()
    };
    let blocks_v2: Vec<CarBlock> = stream_v2.try_collect().await.unwrap();
    assert_eq!(blocks_v1, blocks_v2);

    let stream_v2_with_f3 = {
        let mut snap_bytes: Vec<u8> = vec![];
        let f3 = {
            let mut data = vec![0; 1024 * 4];
            forest_rng().fill(&mut data[..]);
            let cid = crate::f3::snapshot::get_f3_snapshot_cid(&mut data.as_slice()).unwrap();
            Some((cid, Cursor::new(data)))
        };
        crate::chain::export_v2::<Sha256, _>(&db, f3, &head, 0, &mut snap_bytes, None)
            .await
            .unwrap();
        CarStream::new_with_header_v2(Cursor::new(snap_bytes), None)
            .await
            .unwrap()
    };
    let blocks_v2_with_f3: Vec<CarBlock> = stream_v2_with_f3.try_collect().await.unwrap();
    assert_eq!(blocks_v2, blocks_v2_with_f3);
}
