// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use async_std::sync::RwLock;
use forest_libp2p::Multiaddr;
use once_cell::sync::Lazy;
use std::env;

use rpc_api::{API_INFO_KEY, DEFAULT_MULTIADDRESS};

mod auth_ops;
mod chain_ops;
mod client;
mod wallet_ops;

pub use self::auth_ops::*;
pub use self::chain_ops::*;
pub use self::client::*;
pub use self::wallet_ops::*;

pub const DEFAULT_URL: &str = "http://127.0.0.1:1234/rpc/v0";
pub const DEFAULT_PROTOCOL: &str = "http";
pub const DEFAULT_HOST: &str = "127.0.0.1";
pub const DEFAULT_PORT: &str = "1234";
pub const RPC_ENDPOINT: &str = "rpc/v0";

pub struct ApiInfo {
    pub multiaddr: Multiaddr,
    pub token: Option<String>,
}

pub static API_INFO: Lazy<RwLock<ApiInfo>> = Lazy::new(|| {
    // Get API_INFO environment variable if exists, otherwise, use default multiaddress
    let api_info = env::var(API_INFO_KEY).unwrap_or_else(|_| DEFAULT_MULTIADDRESS.to_owned());

    let (multiaddr, token) = match api_info.split_once(':') {
        // Typically this is when a JWT was provided
        Some((jwt, host)) => (
            host.parse().expect("Parse multiaddress"),
            Some(jwt.to_owned()),
        ),
        // Use entire API_INFO env var as host string
        None => (api_info.parse().expect("Parse multiaddress"), None),
    };

    RwLock::new(ApiInfo { multiaddr, token })
});
