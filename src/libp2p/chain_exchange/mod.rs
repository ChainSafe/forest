// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod behaviour;
mod message;
mod provider;
pub use behaviour::*;

pub use self::{message::*, provider::*};
use super::rpc::CborRequestResponse;

/// Libp2p protocol name for `ChainExchange`.
pub const CHAIN_EXCHANGE_PROTOCOL_NAME: &str = "/fil/chain/xchg/0.0.1";

/// `ChainExchange` protocol codec to be used within the RPC service.
pub type ChainExchangeCodec =
    CborRequestResponse<&'static str, ChainExchangeRequest, ChainExchangeResponse>;
