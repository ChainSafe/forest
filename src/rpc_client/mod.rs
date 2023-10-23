// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod auth_ops;
pub mod chain_ops;
pub mod common_ops;
pub mod db_ops;
pub mod mpool_ops;
pub mod net_ops;
pub mod node_ops;
pub mod progress_ops;
pub mod state_ops;
pub mod sync_ops;
pub mod wallet_ops;

use std::env;
use std::fmt;
use std::marker::PhantomData;
use std::str::FromStr;

use crate::libp2p::{Multiaddr, Protocol};
use crate::lotus_json::HasLotusJson;
use crate::lotus_json::LotusJson;
use crate::utils::net::global_http_client;
use jsonrpc_v2::{Error, Id, RequestObject, V2};
use once_cell::sync::Lazy;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tracing::debug;

pub const API_INFO_KEY: &str = "FULLNODE_API_INFO";
pub const DEFAULT_HOST: &str = "127.0.0.1";
pub const DEFAULT_MULTIADDRESS: &str = "/ip4/127.0.0.1/tcp/2345/http";
pub const DEFAULT_PORT: u16 = 2345;
pub const DEFAULT_PROTOCOL: &str = "http";
pub const RPC_ENDPOINT: &str = "rpc/v0";

pub use self::{
    auth_ops::*, chain_ops::*, common_ops::*, mpool_ops::*, net_ops::*, state_ops::*, sync_ops::*,
    wallet_ops::*,
};

#[derive(Clone, Debug)]
pub struct ApiInfo {
    pub multiaddr: Multiaddr,
    pub token: Option<String>,
}

impl fmt::Display for ApiInfo {
    /// Convert an ApiInfo to a string
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(token) = &self.token {
            token.fmt(f)?;
            write!(f, ":")?;
        }
        self.multiaddr.fmt(f)?;
        Ok(())
    }
}

impl FromStr for ApiInfo {
    type Err = multiaddr::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (multiaddr, token) = match s.split_once(':') {
            // token:host
            Some((jwt, host)) => (host.parse()?, Some(jwt.to_owned())),
            // host
            None => (s.parse()?, None),
        };

        Ok(ApiInfo { multiaddr, token })
    }
}

impl ApiInfo {
    // Update API handle with new (optional) token
    pub fn set_token(self, token: Option<String>) -> Self {
        ApiInfo {
            token: token.or(self.token),
            ..self
        }
    }

    // Get API_INFO environment variable if exists, otherwise, use default
    // multiaddress. Fails if the environment variable is malformed.
    pub fn from_env() -> Result<Self, multiaddr::Error> {
        let api_info = env::var(API_INFO_KEY).unwrap_or_else(|_| DEFAULT_MULTIADDRESS.to_owned());
        ApiInfo::from_str(&api_info)
    }

    /// Utility method for sending RPC requests over HTTP
    pub async fn call<P, R>(&self, method_name: &str, params: P) -> Result<R, Error>
    where
        P: Serialize,
        R: DeserializeOwned,
    {
        let rpc_req = RequestObject::request()
            .with_method(method_name)
            .with_params(serde_json::to_value(params)?)
            .with_id(0)
            .finish();

        let api_url = multiaddress_to_url(&self.multiaddr);

        debug!("Using JSON-RPC v2 HTTP URL: {}", api_url);

        let request = global_http_client().post(api_url).json(&rpc_req);
        let request = match self.token.as_ref() {
            Some(token) => request.header(http::header::AUTHORIZATION, token),
            _ => request,
        };

        let rpc_res = request.send().await?.error_for_status()?.json().await?;

        match rpc_res {
            JsonRpcResponse::Result { result, .. } => Ok(result),
            JsonRpcResponse::Error { error, .. } => Err(Error::Full {
                data: None,
                code: error.code,
                message: error.message,
            }),
        }
    }

    // HTTP error
    // JsonRpcError
    // JSON parsing error
    pub async fn call_req<T: HasLotusJson>(&self, req: RpcRequest<T>) -> Result<T, Error> {
        let rpc_req = RequestObject::request()
            .with_method(req.method_name)
            .with_params(req.params)
            .with_id(0)
            .finish();

        let api_url = multiaddress_to_url(&self.multiaddr);

        debug!("Using JSON-RPC v2 HTTP URL: {}", api_url);

        let request = global_http_client().post(api_url).json(&rpc_req);
        let request = match self.token.as_ref() {
            Some(token) => request.header(http::header::AUTHORIZATION, token),
            _ => request,
        };

        let rpc_res: JsonRpcResponse<T::LotusJson> =
            request.send().await?.error_for_status()?.json().await?;

        match rpc_res {
            JsonRpcResponse::Result { result, .. } => Ok(HasLotusJson::from_lotus_json(result)),
            JsonRpcResponse::Error { error, .. } => Err(Error::Full {
                data: None,
                code: error.code,
                message: error.message,
            }),
        }
    }

    pub async fn call_req_e<T: HasLotusJson>(&self, req: RpcRequest<T>) -> Result<T, JsonRpcError> {
        let rpc_req = RequestObject::request()
            .with_method(req.method_name)
            .with_params(req.params)
            .with_id(0)
            .finish();

        let api_url = multiaddress_to_url(&self.multiaddr);

        debug!("Using JSON-RPC v2 HTTP URL: {}", api_url);

        let request = global_http_client().post(api_url).json(&rpc_req);
        let request = match self.token.as_ref() {
            Some(token) => request.header(http::header::AUTHORIZATION, token),
            _ => request,
        };

        let rpc_res: JsonRpcResponse<T::LotusJson> =
            request.send().await?.error_for_status()?.json().await?;

        match rpc_res {
            JsonRpcResponse::Result { result, .. } => Ok(HasLotusJson::from_lotus_json(result)),
            JsonRpcResponse::Error { error, .. } => Err(error),
        }
    }
}

pub static API_INFO: Lazy<ApiInfo> = Lazy::new(|| {
    // Get API_INFO environment variable if exists, otherwise, use default
    // multiaddress
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

    ApiInfo { multiaddr, token }
});

/// Error object in a response
#[derive(Debug, Deserialize)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
}

impl std::fmt::Display for JsonRpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} (code={})", self.message, self.code)
    }
}

impl std::error::Error for JsonRpcError {
    fn description(&self) -> &str {
        &self.message
    }
}

impl From<reqwest::Error> for JsonRpcError {
    fn from(reqwest_error: reqwest::Error) -> Self {
        JsonRpcError {
            code: reqwest_error
                .status()
                .map(|s| s.as_u16())
                .unwrap_or_default() as i64,
            message: reqwest_error.to_string(),
        }
    }
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
    port: u16,
    host: String,
}

/// Parses a multi-address into a URL
fn multiaddress_to_url(multiaddr: &Multiaddr) -> String {
    // Fold Multiaddress into a Url struct
    let addr = multiaddr.iter().fold(
        Url {
            protocol: DEFAULT_PROTOCOL.to_owned(),
            port: DEFAULT_PORT,
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
                    addr.port = p;
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
async fn call<P, R>(method_name: &str, params: P, token: &Option<String>) -> Result<R, Error>
where
    P: Serialize,
    R: DeserializeOwned,
{
    API_INFO
        .clone()
        .set_token(token.clone())
        .call(method_name, params)
        .await
}

/// Utility method for sending RPC requests over HTTP
async fn call_req<R: HasLotusJson>(req: RpcRequest<R>, token: &Option<String>) -> Result<R, Error> {
    API_INFO
        .clone()
        .set_token(token.clone())
        .call_req(req)
        .await
}

#[derive(Debug, Clone)]
pub struct RpcRequest<T = serde_json::Value> {
    pub method_name: &'static str,
    params: serde_json::Value,
    result_type: PhantomData<T>,
}

impl<T> RpcRequest<T> {
    pub fn new<P: HasLotusJson>(method_name: &'static str, params: P) -> Self {
        RpcRequest {
            method_name,
            params: serde_json::to_value(HasLotusJson::into_lotus_json(params)).unwrap_or(
                serde_json::Value::String(
                    "INTERNAL ERROR: Parameters could not be serialized as JSON".to_string(),
                ),
            ),
            result_type: PhantomData,
        }
    }

    pub fn lower(self) -> RpcRequest {
        RpcRequest {
            method_name: self.method_name,
            params: self.params,
            result_type: PhantomData,
        }
    }
}
