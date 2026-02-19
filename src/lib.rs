// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#![recursion_limit = "1024"]
#![cfg_attr(
    not(test),
    deny(
        clippy::todo,
        clippy::dbg_macro,
        clippy::indexing_slicing,
        clippy::get_unwrap
    )
)]
#![cfg_attr(
    doc,
    deny(rustdoc::all),
    allow(
        // We build with `--document-private-items` on both docs.rs and our
        // vendored docs.
        rustdoc::private_intra_doc_links,
        // See module `doctest_private` below.
        rustdoc::private_doc_tests,
        rustdoc::missing_crate_level_docs
    )
)]

cfg_if::cfg_if! {
    if #[cfg(feature = "rustalloc")] {
    } else if #[cfg(feature = "jemalloc")] {
        use crate::cli_shared::tikv_jemallocator::Jemalloc;
        #[global_allocator]
        static GLOBAL: Jemalloc = Jemalloc;
    } else if #[cfg(feature = "system-alloc")] {
        use std::alloc::System;
        #[global_allocator]
        static GLOBAL: System = System;
    }
}

mod auth;
mod beacon;
mod blocks;
mod chain;
mod chain_sync;
mod cid_collections;
mod cli;
mod cli_shared;
mod daemon;
mod db;
mod dev;
mod documentation;
mod eth;
mod f3;
mod fil_cns;
mod genesis;
mod health;
mod interpreter;
mod ipld;
mod key_management;
mod libp2p;
mod libp2p_bitswap;
mod lotus_json;
mod message;
mod message_pool;
mod metrics;
mod networks;
mod rpc;
mod shim;
mod state_manager;
mod state_migration;
mod statediff;
#[cfg(any(test, doc))]
mod test_utils;
mod tool;
mod utils;
mod wallet;

/// These items are semver-exempt, and exist for forest author use only
// We want to have doctests, but don't want our internals to be public because:
// - We don't want to be concerned with library compat
//   (We want our cargo semver to be _for the command line_).
// - We don't want to mistakenly export items which we never actually use.
//
// So we re-export the relevant items and test with `cargo test --doc --features doctest-private`
#[cfg(feature = "doctest-private")]
#[doc(hidden)]
pub mod doctest_private {
    pub use crate::{
        blocks::{CachingBlockHeader, Ticket, TipsetKey},
        cli::humantoken::{TokenAmountPretty, parse},
        shim::{
            address::Address, crypto::Signature, econ::TokenAmount, error::ExitCode,
            randomness::Randomness, sector::RegisteredSealProof, state_tree::ActorState,
            version::NetworkVersion,
        },
        utils::io::progress_log::WithProgress,
        utils::net::{DownloadFileOption, download_to},
        utils::{encoding::blake2b_256, encoding::keccak_256, io::read_toml},
    };
}

/// These items are semver-exempt, and exist for forest author use only
// Allow benchmarks of forest internals
#[cfg(feature = "benchmark-private")]
#[doc(hidden)]
pub mod benchmark_private;

/// These items are semver-exempt, and exist for forest author use only
// Allow interop tests of forest internals
#[cfg(feature = "interop-tests-private")]
#[doc(hidden)]
pub mod interop_tests_private {
    pub mod libp2p {
        pub use crate::libp2p::*;
    }
    pub mod libp2p_bitswap {
        pub use crate::libp2p_bitswap::*;
    }
    pub mod beacon {
        pub use crate::beacon::BeaconEntry;
    }
}

// These should be made private in https://github.com/ChainSafe/forest/issues/3013
pub use auth::{JWT_IDENTIFIER, verify_token};
pub use cli::main::main as forest_main;
pub use cli_shared::cli::{Client, Config};
pub use daemon::main::main as forestd_main;
pub use dev::main::main as forest_dev_main;
pub use key_management::{
    ENCRYPTED_KEYSTORE_NAME, FOREST_KEYSTORE_PHRASE_ENV, KEYSTORE_NAME, KeyStore, KeyStoreConfig,
};
pub use tool::main::main as forest_tool_main;
pub use wallet::main::main as forest_wallet_main;

#[cfg(test)]
fn block_on<T>(f: impl std::future::Future<Output = T>) -> T {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(f)
}
