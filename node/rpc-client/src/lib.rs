// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

/// Filecoin RPC client interface methods
pub mod auth_ops;
pub mod chain_ops;
pub mod mpool_ops;
pub mod net_ops;
pub mod state_ops;
pub mod sync_ops;
pub mod wallet_ops;

use async_std::sync::RwLock;
use forest_libp2p::{Multiaddr, Protocol};
/// Filecoin HTTP JSON-RPC client methods
use jsonrpc_v2::{Error, Id, RequestObject, V2};
use log::{debug, error};
use once_cell::sync::Lazy;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::env;

pub const API_INFO_KEY: &str = "FULLNODE_API_INFO";
pub const DEFAULT_HOST: &str = "127.0.0.1";
pub const DEFAULT_MULTIADDRESS: &str = "/ip4/127.0.0.1/tcp/1234/http";
pub const DEFAULT_PORT: &str = "1234";
pub const DEFAULT_PROTOCOL: &str = "http";
pub const DEFAULT_URL: &str = "http://127.0.0.1:1234/rpc/v0";
pub const RPC_ENDPOINT: &str = "rpc/v0";

pub use self::auth_ops::*;
pub use self::chain_ops::*;
pub use self::mpool_ops::*;
pub use self::net_ops::*;
pub use self::state_ops::*;
pub use self::sync_ops::*;
pub use self::wallet_ops::*;

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

/// Error object in a response
#[derive(Deserialize)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
}

#[derive(Deserialize)]
#[serde(untagged)]
pub enum JsonRpcResponse<R> {
    Result {
        jsonrpc: V2,
        result: R,
        id: Id,
    },
    Error {
        jsonrpc: V2,
        error: JsonRpcError,
        id: Id,
    },
}

struct Url {
    protocol: String,
    port: String,
    host: String,
}

/// Parses a multiaddress into a Url
fn multiaddress_to_url(multiaddr: Multiaddr) -> String {
    // Fold Multiaddress into a Url struct
    let addr = multiaddr.into_iter().fold(
        Url {
            protocol: DEFAULT_PROTOCOL.to_owned(),
            port: DEFAULT_PORT.to_owned(),
            host: DEFAULT_HOST.to_owned(),
        },
        |mut addr, protocol| {
            match protocol {
                Protocol::Ip6(ip) => {
                    addr.host = ip.to_string();
                }
                Protocol::Ip4(ip) => {
                    addr.host = ip.to_string();
                }
                Protocol::Dns(dns) => {
                    addr.host = dns.to_string();
                }
                Protocol::Dns4(dns) => {
                    addr.host = dns.to_string();
                }
                Protocol::Dns6(dns) => {
                    addr.host = dns.to_string();
                }
                Protocol::Dnsaddr(dns) => {
                    addr.host = dns.to_string();
                }
                Protocol::Tcp(p) => {
                    addr.port = p.to_string();
                }
                Protocol::Http => {
                    addr.protocol = "http".to_string();
                }
                Protocol::Https => {
                    addr.protocol = "https".to_string();
                }
                _ => {}
            };
            addr
        },
    );

    // Format, print and return the URL
    let url = format!(
        "{}://{}:{}/{}",
        addr.protocol, addr.host, addr.port, RPC_ENDPOINT
    );

    url
}

/// Utility method for sending RPC requests over HTTP
async fn call<P, R>(method_name: &str, params: P) -> Result<R, Error>
where
    P: Serialize,
    R: DeserializeOwned,
{
    let rpc_req = RequestObject::request()
        .with_method(method_name)
        .with_params(serde_json::to_value(params)?)
        .finish();

    let api_info = API_INFO.read().await;
    let api_url = multiaddress_to_url(api_info.multiaddr.to_owned());

    debug!("Using JSON-RPC v2 HTTP URL: {}", api_url);

    // Split the JWT off if present, format multiaddress as URL, then post RPC request to URL
    let mut http_res = match api_info.token.to_owned() {
        Some(jwt) => surf::post(api_url)
            .content_type("application/json-rpc")
            .body(surf::Body::from_json(&rpc_req)?)
            .header("Authorization", jwt),
        None => surf::post(api_url)
            .content_type("application/json-rpc")
            .body(surf::Body::from_json(&rpc_req)?),
    }
    .await?;

    let res = http_res.body_string().await?;

    let code = http_res.status() as i64;

    if code != 200 {
        return Err(Error::Full {
            message: format!("Error code from HTTP Response: {}", code),
            code,
            data: None,
        });
    }

    // Return the parsed RPC result
    let rpc_res: JsonRpcResponse<R> = match serde_json::from_str(&res) {
        Ok(r) => r,
        Err(e) => {
            let err = format!(
                "Parse Error: Response from RPC endpoint could not be parsed. Error was: {}",
                e
            );
            error!("{}", &err);
            return Err(err.into());
        }
    };

    match rpc_res {
        JsonRpcResponse::Result { result, .. } => Ok(result),
        JsonRpcResponse::Error { error, .. } => Err(Error::Full {
            data: None,
            code: error.code,
            message: error.message,
        }),
    }
}
