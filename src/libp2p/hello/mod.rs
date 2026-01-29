// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod message;
pub use self::message::*;
mod behaviour;
pub use behaviour::*;
mod codec;
use codec::*;

/// Libp2p Hello protocol name.
pub const HELLO_PROTOCOL_NAME: &str = "/fil/hello/1.0.0";
