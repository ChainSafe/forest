// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use cid::Cid;
use Code::Blake2b256;
use rand::{thread_rng, Rng};
use std::iter;

pub fn random_bytes(len: usize) -> Vec<u8> {
    let mut rng = thread_rng();
    iter::repeat_with(|| rng.gen()).take(len).collect()
}

pub fn random_blocks(len: usize, block_size: usize) -> (Vec<Vec<u8>>, Vec<Cid>) {
    let blocks: Vec<_> = iter::repeat_with(|| random_bytes(block_size))
        .take(len)
        .collect();
    let links = blocks
        .iter()
        .map(|block| Cid::new_from_cbor(block, Blake2b256))
        .collect();
    (blocks, links)
}

pub fn random_cid() -> Cid {
    Cid::new_from_cbor(&random_bytes(16), Blake2b256)
}
