// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use async_std::fs::File;
use async_std::io::BufReader;
use db::MemoryDB;
use forest_car::*;

#[async_std::test]
async fn load_into_blockstore() {
    let file = File::open("tests/test.car").await.unwrap();
    let buf_reader = BufReader::new(file);
    let mut bs = MemoryDB::default();

    let _ = load_car(&mut bs, buf_reader).await.unwrap();
}
