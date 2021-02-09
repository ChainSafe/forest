// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#![recursion_limit = "1024"]

mod bad_block_cache;
mod bucket;
mod errors;
mod network_context;
mod peer_manager;
mod sync;
mod sync_state;
mod sync_worker;

// workaround for a compiler bug, see https://github.com/rust-lang/rust/issues/55779
extern crate serde;

pub use self::bad_block_cache::BadBlockCache;
pub use self::errors::Error;
pub use self::sync::{compute_msg_meta, ChainSyncer, SyncConfig};
pub use self::sync_state::{SyncStage, SyncState};
