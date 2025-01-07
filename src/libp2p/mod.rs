// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod behaviour;
pub mod chain_exchange;
mod config;
pub mod discovery;
mod gossip_params;
pub mod hello;
pub mod keypair;
pub mod metrics;
mod peer_manager;
pub mod ping;
pub mod rpc;
mod service;

// Re-export some libp2p types
pub use cid::multihash::Multihash;
pub use libp2p::{
    identity::{ed25519, Keypair, ParseError, PeerId},
    multiaddr::{Multiaddr, Protocol},
};

pub(in crate::libp2p) use self::behaviour::*;
pub use self::{config::*, peer_manager::*, service::*};
#[cfg(test)]
mod tests {
    mod decode_test;
}
