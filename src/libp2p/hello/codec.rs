// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::libp2p::rpc::CborRequestResponse;

/// Libp2p Hello protocol name.
pub const HELLO_PROTOCOL_NAME: &str = "/fil/hello/1.0.0";

/// Hello protocol codec to be used within the RPC service.
pub type HelloCodec = CborRequestResponse<&'static str, HelloRequest, HelloResponse>;
