// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod binding {
    #![allow(warnings)]
    #![allow(clippy::indexing_slicing)]
    rust2go::r2g_include_binding!();
}

#[rust2go::r2g]
pub trait GoF3Node {
    fn run(
        rpc_endpoint: String,
        f3_rpc_socket_address: String,
        finality: i64,
        db: String,
        manifest_server: String,
    ) -> bool;
}
