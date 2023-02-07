// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#![doc = include_str!("../README.md")]

use std::io::Result as IOResult;

use futures::prelude::*;
use libipld::{cid::Cid, prelude::*};
use prost::Message;
use tracing::*;

mod proto;

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

pub mod task {
    //! Re-exports API(s) from the chosen task library

    cfg_if::cfg_if! {
        if #[cfg(feature = "tokio")] {
            pub use tokio::{spawn, task::spawn_blocking, time::{sleep, timeout}};
        } else {
            pub use async_std::{future::timeout, task::{spawn, sleep}};
            pub use compat::spawn_blocking_compat as spawn_blocking;

            mod compat {
                use std::convert::Infallible;

                pub async fn spawn_blocking_compat<F, T>(f: F) -> Result<T, Infallible>
                where
                    F: FnOnce() -> T + Send + 'static,
                    T: Send + 'static,
                {
                    Ok(async_std::task::spawn_blocking(f).await)
                }
            }
        }
    }
}
