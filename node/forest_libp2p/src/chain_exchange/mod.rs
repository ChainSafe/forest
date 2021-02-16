// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod message;
mod provider;

pub use self::message::*;
pub use self::provider::*;
use super::rpc::CborRequestResponse;
use libp2p::core::ProtocolName;

/// Libp2p protocol ID for ChainExchange.
pub const CHAIN_XCHG_PROTOCOL_ID: &[u8] = b"/fil/chain/xchg/0.0.1";

/// Type to satisfy `ProtocolName` interface for ChainExchange RPC.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct ChainExchangeProtocolName;

impl ProtocolName for ChainExchangeProtocolName {
    fn protocol_name(&self) -> &[u8] {
        CHAIN_XCHG_PROTOCOL_ID
    }
}

/// ChainExchange protocol codec to be used within the RPC service.
pub type ChainExchangeCodec =
    CborRequestResponse<ChainExchangeProtocolName, ChainExchangeRequest, ChainExchangeResponse>;
