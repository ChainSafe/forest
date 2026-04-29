// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::time::Duration;

use super::*;
use crate::libp2p::rpc::{CborRequestResponse, CodecConfig};

/// Codec limits for the Hello protocol.
///
/// - Request: tipset CIDs + height + weight + genesis CID — comfortably under
///   1 KiB even at the 15-blocks-per-tipset ceiling. 4 KiB cap.
/// - Response: `[u64, u64]`, at most **19 bytes** CBOR-encoded. 32 byte cap.
/// - Decode timeout: 10 seconds — the response is tiny, anything stalling longer is
///   misbehaving.
pub struct HelloCodecConfig;

impl CodecConfig for HelloCodecConfig {
    const MAX_REQUEST_BYTES: usize = 4096;
    const MAX_RESPONSE_BYTES: usize = 32;
    const DECODE_TIMEOUT: Duration = Duration::from_secs(10);
}

pub type HelloCodec =
    CborRequestResponse<&'static str, HelloRequest, HelloResponse, HelloCodecConfig>;
