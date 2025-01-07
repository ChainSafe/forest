// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod binding {
    #![allow(warnings)]
    #![allow(clippy::indexing_slicing)]
    rust2go::r2g_include_binding!();
}

#[rust2go::r2g]
pub trait GoKadNode {
    fn run();

    fn connect(multiaddr: &String);

    fn get_n_connected() -> usize;
}

#[rust2go::r2g]
pub trait GoBitswapNode {
    fn run();

    fn connect(multiaddr: &String);

    fn get_block(cid: &String) -> bool;
}
