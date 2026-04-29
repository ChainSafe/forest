// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod behaviour;
mod message;
mod provider;
use std::time::Duration;

pub use behaviour::*;

pub use self::{message::*, provider::*};
use super::rpc::{CborRequestResponse, CodecConfig};

/// Libp2p protocol name for `ChainExchange`.
pub const CHAIN_EXCHANGE_PROTOCOL_NAME: &str = "/fil/chain/xchg/0.0.1";

/// Codec limits for the `ChainExchange` protocol.
///
/// - Request: tipset CIDs + length + options bitfield — well under 1 KiB. 4 KiB cap.
/// - Response: cap matches Lotus's [`maxExchangeMessageSize`] (15 blocks × 8 MiB messages).
/// - Decode timeout: 60s — accommodates ~32 MiB realistic responses at
///   ~5 Mbps per stream (we run up to 3 outbound chain-exchange streams in
///   parallel, so per-stream bandwidth is a fraction of the peer's link).
///
/// [`maxExchangeMessageSize`]: https://github.com/filecoin-project/lotus/blob/v1.35.1/chain/exchange/client.go#L30
pub struct ChainExchangeCodecConfig;

impl CodecConfig for ChainExchangeCodecConfig {
    const MAX_REQUEST_BYTES: usize = 4096;
    const MAX_RESPONSE_BYTES: usize = 120 * 1024 * 1024;
    const DECODE_TIMEOUT: Duration = Duration::from_secs(60);
}

pub type ChainExchangeCodec = CborRequestResponse<
    &'static str,
    ChainExchangeRequest,
    ChainExchangeResponse,
    ChainExchangeCodecConfig,
>;
