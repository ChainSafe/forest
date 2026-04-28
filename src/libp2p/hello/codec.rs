// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::libp2p::rpc::CborRequestResponse;

/// Hello protocol codec to be used within the RPC service.
///
/// `HelloResponse` is `[u64, u64]` — at most **19 bytes** CBOR-encoded
/// (1-byte array header + two 9-byte `u64`s for `u64::MAX`).
pub type HelloCodec = CborRequestResponse<&'static str, HelloRequest, HelloResponse, 32>;
