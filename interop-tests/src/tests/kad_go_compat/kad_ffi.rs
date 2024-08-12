// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod binding {
    #![allow(warnings)]
    rust2go::r2g_include_binding!();
}

#[derive(rust2go::R2G, Clone, Default)]
pub struct EmptyReq {}

#[rust2go::r2g]
pub trait GoKadNode {
    fn run();

    fn connect(multiaddr: String);

    fn get_n_connected(req: &EmptyReq) -> usize;
}
