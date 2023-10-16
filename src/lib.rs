// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! We here document two fundamental logical components of Filecoin:
//! 1. The blockchain
//! 2. The state
//!
//! Filecoin implementations must store these as the `ChainStore` and the `StateStore`
//! as referenced [in the filecoin spec](https://github.com/filecoin-project/specs/blob/936f07f9a444036fe86442c919940ea0e4fb0a0b/content/systems/filecoin_nodes/repository/ipldstore/_index.md?plain=1#L43-L50).
//!
//! # The Filecoin blockchain
//!
//! Filecoin consists of a blockchain of messages.
//! These are the core objects for the blockchain.
//! Each one can be addressed by a [`Cid`](cid::Cid).
//!
//! - [`Message`](shim::message::Message)s are statements of interactions between
//!   a small number[^1] of actors on the blockchain.
//!   They describe and (equivalently) represent a change in _the blockchain state_.
//!   See [`apply_block_messages`](state_manager::apply_block_messages) to learn
//!   more.
//!   Messages may be [signed](message::SignedMessage).
//!   TODO(aatifsyed): by whom? what does that mean?
//! - `Message`s are grouped into [`Block`](blocks::Block)s, with a single
//!   [`BlockHeader`](blocks::BlockHeader).
//!   These are what are mined by miners to get `FIL` (money).
//!   They define an [_epoch_](blocks::BlockHeader::epoch) and a
//!   [_parent tipset_](blocks::BlockHeader::parents).
//!   The _epoch_ is a monotonically increasing number from `0` (genesis).
//! - `Block`s are grouped into [`Tipset`](blocks::Tipset)s.
//!   All blocks in a tipset share the same `epoch`.
//!
//! [^1]: We'll mostly concern ourselves with the [built-in actors](https://docs.filecoin.io/basics/the-blockchain/actors#built-in-actors).
//!       They include e.g user accounts.
//!
//! ```text
//!     ┌───────────────────────────────┐
//!     │ BlockHeader { epoch:  0, .. } │ //  The genesis block/tipset
//!   ┌●└───────────────────────────────┘
//!   ~
//!   └─┬───────────────────────────────┐
//!     │ BlockHeader { epoch: 10, .. } │ // The epoch 10 tipset - one block with two messages
//!   ┌●└┬──────────────────────────────┘
//!   │  │
//!   │  │ "I contain the following messages..."
//!   │  │
//!   │  ├──────────────────┐
//!   │  │ ┌──────────────┐ │ ┌───────────────────┐
//!   │  └►│ Message:     │ └►│ Message:          │
//!   │    │  Afri -> Bob │   │  Charlie -> David │
//!   │    └──────────────┘   └───────────────────┘
//!   │
//!   │ "my parent is..."
//!   │
//!   └─┬───────────────────────────────┐
//!     │ BlockHeader { epoch: 11, .. } │ // The epoch 11 tipset - one block with one message
//!   ┌●└┬──────────────────────────────┘
//!   │  │ ┌────────────────┐
//!   │  └►│ Message:       │
//!   │    │  Eric -> Frank │
//!   │    └────────────────┘
//!   │
//!   │ // the epoch 12 tipset - two blocks, with a total of 3 messages
//!   │
//!   ├───────────────────────────────────┐
//!   └─┬───────────────────────────────┐ └─┬───────────────────────────────┐
//!     │ BlockHeader { epoch: 12, .. } │   │ BlockHeader { epoch: 12, .. } │
//!   ┌●└┬──────────────────────────────┘   └┬─────────────────────┬────────┘
//!   ~  │ ┌───────────────────────┐         │ ┌─────────────────┐ │ ┌──────────────┐
//!      └►│ Message:              │         └►│ Message:        │ └►│ Message:     │
//!        │  Guillaume -> Hailong │           │  Hubert -> Ivan │   │  Josh -> Kai │
//!        └───────────────────────┘           └─────────────────┘   └──────────────┘
//! ```
//!
//! The [`ChainMuxer`](chain_sync::ChainMuxer) receives two kinds of [messages](libp2p::PubsubMessage)
//! from peers:
//! - [`GossipBlock`](blocks::GossipBlock)s are descriptions of a single block, with the `BlockHeader` and `Message` CIDs.
//! - [`SignedMessage`](message::SignedMessage)s
//!
//! It assembles these messages into a chain to genesis.

#![recursion_limit = "1024"]
#![cfg_attr(not(test), deny(clippy::todo, clippy::dbg_macro))]
#![cfg_attr(
    doc,
    deny(rustdoc::all),
    allow(
        // We build with `--document-private-items` on both docs.rs and our
        // vendored docs.
        rustdoc::private_intra_doc_links,
        // See module `doctest_private` below.
        rustdoc::private_doc_tests,
        // TODO(aatifsyed): https://github.com/ChainSafe/forest/issues/3602
        rustdoc::missing_crate_level_docs
    )
)]

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
mod cid_collections;
mod cli;
mod cli_shared;
mod daemon;
mod db;
mod fil_cns;
mod genesis;
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
mod rpc_api;
mod rpc_client;
mod shim;
mod state_manager;
mod state_migration;
mod statediff;
#[cfg(test)]
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
        blocks::{BlockHeader, Ticket, TipsetKeys},
        cli::humantoken::{parse, TokenAmountPretty},
        shim::{
            address::Address, crypto::Signature, econ::TokenAmount, error::ExitCode,
            randomness::Randomness, sector::RegisteredSealProof, state_tree::ActorState,
            version::NetworkVersion,
        },
        utils::io::progress_log::WithProgress,
        utils::{encoding::blake2b_256, io::read_toml},
    };
}

/// These items are semver-exempt, and exist for forest author use only
// Allow benchmarks of forest internals
#[cfg(feature = "benchmark-private")]
#[doc(hidden)]
pub mod benchmark_private {
    pub use crate::utils::cid;
    pub use crate::utils::db::car_index;
}

// These should be made private in https://github.com/ChainSafe/forest/issues/3013
pub use auth::{verify_token, JWT_IDENTIFIER};
pub use cli::main::main as forest_main;
pub use cli_shared::cli::{Client, Config};
pub use daemon::main::main as forestd_main;
pub use key_management::{
    KeyStore, KeyStoreConfig, ENCRYPTED_KEYSTORE_NAME, FOREST_KEYSTORE_PHRASE_ENV, KEYSTORE_NAME,
};
pub use tool::main::main as forest_tool_main;
pub use wallet::main::main as forest_wallet_main;
