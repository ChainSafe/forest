// Copyright 2019-2026 ChainSafe Systems
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
///
/// Cap matches Lotus's [`maxExchangeMessageSize`] (15 blocks × 8 MiB messages).
///
/// [`maxExchangeMessageSize`]: https://github.com/filecoin-project/lotus/blob/v1.35.1/chain/exchange/client.go#L30
pub type ChainExchangeCodec = CborRequestResponse<
    &'static str,
    ChainExchangeRequest,
    ChainExchangeResponse,
    { 120 * 1024 * 1024 },
>;
