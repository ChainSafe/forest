// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#![recursion_limit = "1024"]
#![allow(
    deprecated,
    unused,
    clippy::upper_case_acronyms,
    clippy::enum_variant_names,
    clippy::module_inception
)] // # 2991

cfg_if::cfg_if! {
    if #[cfg(feature = "rustalloc")] {
    } else if #[cfg(feature = "mimalloc")] {
        use crate::cli_shared::mimalloc::MiMalloc;
        #[global_allocator]
        static GLOBAL: MiMalloc = MiMalloc;
    } else if #[cfg(feature = "jemalloc")] {
        use crate::cli_shared::tikv_jemallocator::Jemalloc;
        #[global_allocator]
        static GLOBAL: Jemalloc = Jemalloc;
    }
}

mod auth;
mod beacon;
mod blocks;
mod chain;
mod chain_sync;
mod cli;
mod cli_shared;
mod daemon;
mod db;
mod deleg_cns;
mod fil_cns;
mod genesis;
mod interpreter;
mod ipld;
mod json;
mod key_management;
mod libp2p;
mod libp2p_bitswap;
mod message;
mod message_pool;
mod metrics;
mod networks;
mod rpc;
mod rpc_api;
mod rpc_client;
mod shim;
mod state_manager;
mod state_migration;
mod statediff;
mod test_utils;
mod utils;

pub use auth::{verify_token, JWT_IDENTIFIER};
pub use cli::main::main as forest_main;
pub use cli_shared::cli::{Client, Config};
pub use daemon::main::main as forestd_main;
pub use key_management::{
    KeyStore, KeyStoreConfig, ENCRYPTED_KEYSTORE_NAME, FOREST_KEYSTORE_PHRASE_ENV, KEYSTORE_NAME,
};
