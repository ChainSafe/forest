// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use db::MemoryDB;
use forest_car::*;
use std::fs::File;
use std::io::BufReader;

#[test]
fn load_into_blockstore() {
    let file = File::open("tests/devnet.car").unwrap();
    let buf_reader = BufReader::new(file);
    let mut bs = MemoryDB::default();

    let _ = load_car(&mut bs, buf_reader).unwrap();
}
