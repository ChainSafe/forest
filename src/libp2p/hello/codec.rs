// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::libp2p::rpc::CborRequestResponse;

/// Hello protocol codec to be used within the RPC service.
pub type HelloCodec = CborRequestResponse<&'static str, HelloRequest, HelloResponse>;
