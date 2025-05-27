// Copyright 2019-2025 ChainSafe Systems
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
        jwt: String,
        f3_rpc_endpoint: String,
        initial_power_table: String,
        bootstrap_epoch: i64,
        finality: i64,
        f3_root: String,
    ) -> bool;
}
