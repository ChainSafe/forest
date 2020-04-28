// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use db::MemoryDB;
use forest_car::*;
use std::fs::File;
use std::io::BufReader;
use blockstore::BlockStore;

#[test]
fn load_into_blockstore() {
    let file = File::open("tests/devnet.car").unwrap();
    let buf_reader = BufReader::new(file);
    let mut bs = MemoryDB::default();

    let cids = load_car(&mut bs, buf_reader).unwrap();

    println!("{:x?}", bs.get_bytes(&cids[0]).unwrap())
}
