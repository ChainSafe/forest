// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! This module implements network version 12 or actorv4 state migration
//! Please read https://filecoin.io/blog/posts/filecoin-network-v12/
//! to learn more about network version 12 migration.
//! This is more or less a direct port of the state migration
//! implemented in lotus' specs-actors library.

pub mod miner;

pub use miner::miner_migrator_v4;
