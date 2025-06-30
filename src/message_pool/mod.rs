// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
mod block_prob;
mod config;
mod errors;
mod msg_chain;
mod msgpool;

pub use self::{
    config::*,
    errors::*,
    msgpool::{msg_pool::MessagePool, provider::MpoolRpcProvider, *},
};

pub use block_prob::block_probabilities;

// In src/message_pool/mod.rs
use crate::message::SignedMessage;
use crate::shim::address::Address;

#[derive(Debug, Clone)]
pub enum MpoolEvent {
    Add(SignedMessage),
    Remove { from: Address, nonce: u64 },
}
