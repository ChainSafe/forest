// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#![recursion_limit = "1024"]

mod behaviour;
pub mod chain_exchange;
mod config;
mod discovery;
mod gossip_params;
pub mod hello;
mod metrics;
mod peer_manager;
pub mod rpc;
mod service;

pub(crate) use self::behaviour::*;
pub use self::config::*;
pub use self::peer_manager::*;
pub use self::service::*;

// Re-export some libp2p types
pub use libp2p::core::PeerId;
pub use libp2p::identity::{ed25519, Keypair};
pub use libp2p::multiaddr::{Multiaddr, Protocol};
pub use multihash::Multihash;
