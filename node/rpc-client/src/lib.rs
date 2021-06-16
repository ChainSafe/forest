// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod auth_ops;
mod chain_ops;
mod client;
mod wallet_ops;

pub use self::auth_ops::*;
pub use self::chain_ops::*;
pub use self::client::*;
pub use self::wallet_ops::*;

pub const DEFAULT_MULTIADDRESS: &str = "/ip4/127.0.0.1/tcp/1234/http";
pub const API_INFO_KEY: &str = "FULLNODE_API_INFO";
