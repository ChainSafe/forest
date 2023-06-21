// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
//! Another libp2p
//! bitswap([SPEC](https://github.com/ipfs/specs/blob/main/BITSWAP.md))
//! implementation in Rust.
//!
//! ## Features
//!
//! - Compatible with [`go-bitswap`](https://github.com/ipfs/go-bitswap)
//! - Optional request manager
//! - Prometheus metrics
//!
//! ## Usage
//!
//! Basic usage of `BitswapBehaviour`, for writing swarm event flow, sending or
//! receiving a request or a response, checkout `tests/go_compat.rs`. Note that a
//! request manager is needed for a real-world application.
//!
//! To use the builtin request manager that is optimized for Filecoin network, a
//! data store that implements `BitswapStoreRead` and `BitswapStoreReadWrite` is
//! required. For hooking request manager in swarm event flow, requesting a block
//! via request manager API, checkout `tests/request_manager.rs`.

use std::io::Result as IOResult;

use futures::prelude::*;
use libipld::{cid::Cid, prelude::*};
use tracing::*;

mod internals;
use internals::*;

mod behaviour;
pub use behaviour::*;

mod message;
pub use message::*;

mod metrics;
pub use metrics::register_metrics;

pub mod request_manager;

mod store;
pub use store::*;

mod pb {
    include!(concat!(env!("OUT_DIR"), "/proto/mod.rs"));
}

#[cfg(not(target_arch = "wasm32"))]
pub mod task {
    //! Re-exports API(s) from the chosen task library
    pub use tokio::{
        spawn,
        task::spawn_blocking,
        time::{sleep, timeout},
    };
}
#[cfg(test)]
mod tests {
    mod go_compat;
    mod request_manager;
}
