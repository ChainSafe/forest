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

pub mod auth;
pub mod beacon;
pub mod blocks;
pub mod chain;
pub mod chain_sync;
pub mod cli;
pub mod cli_shared;
pub mod daemon;
pub mod db;
pub mod deleg_cns;
pub mod fil_cns;
pub mod genesis;
pub mod interpreter;
pub mod ipld;
pub mod json;
pub mod key_management;
pub mod libp2p;
pub mod libp2p_bitswap;
pub mod message;
pub mod message_pool;
pub mod metrics;
pub mod networks;
pub mod rpc;
pub mod rpc_api;
pub mod rpc_client;
pub mod shim;
pub mod state_manager;
pub mod state_migration;
pub mod statediff;
pub mod test_utils;
pub mod utils;

pub use auth::{verify_token, JWT_IDENTIFIER};
pub use cli::main::main as forest_main;
pub use cli_shared::cli::{Client, Config};
pub use daemon::main::main as forestd_main;
pub use key_management::{
    KeyStore, KeyStoreConfig, ENCRYPTED_KEYSTORE_NAME, FOREST_KEYSTORE_PHRASE_ENV, KEYSTORE_NAME,
};
