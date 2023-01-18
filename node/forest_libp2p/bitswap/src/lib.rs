// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use futures::prelude::*;
use libipld::{cid::Cid, prelude::*};
use prost::Message;
use std::{fmt::Display, io::Result as IOResult};
use tracing::*;

mod behaviour;
mod codec;
mod event_handlers;
mod message;
mod metrics;
mod prefix;
mod proto;
mod protocol;
mod request_manager;
mod store;

pub use behaviour::*;
pub use event_handlers::*;
pub use message::*;
pub use metrics::*;
pub use protocol::*;
pub use request_manager::*;
pub use store::*;

fn map_io_err(e: impl Display) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::Other, e.to_string())
}
