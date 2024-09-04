// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod binding {
    #![allow(warnings)]
    rust2go::r2g_include_binding!();
}

#[derive(rust2go::R2G, Clone, Default)]
pub struct EmptyReq {}

#[rust2go::r2g]
pub trait GoF3Node {
    fn run(
        rpc_endpoint: String,
        f3_rpc_endpoint: String,
        finality: i64,
        db: String,
        manifest_server: String,
    ) -> bool;
}
