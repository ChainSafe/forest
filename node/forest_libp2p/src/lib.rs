// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#![recursion_limit = "1024"]

#[macro_use]
extern crate lazy_static;

mod behaviour;
pub mod chain_exchange;
mod config;
mod discovery;
mod gossip_params;
pub mod hello;
pub mod rpc;
mod service;

pub(crate) use self::behaviour::*;
pub use self::config::*;
pub use self::service::*;

// Re-export some libp2p types
pub use libp2p::core::PeerId;
pub use libp2p::multiaddr::Multiaddr;
