// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#![doc = include_str!("../README.md")]

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
