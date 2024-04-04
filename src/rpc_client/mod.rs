// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod auth_ops;
pub mod beacon_ops;
pub mod chain_ops;
pub mod common_ops;
pub mod eth_ops;
pub mod mpool_ops;
pub mod net_ops;
pub mod node_ops;
pub mod state_ops;
pub mod sync_ops;
pub mod wallet_ops;

use std::env;
use std::fmt;
use std::marker::PhantomData;
use std::str::FromStr;
use std::time::Duration;

use crate::libp2p::{Multiaddr, Protocol};
use crate::lotus_json::HasLotusJson;
pub use crate::rpc::JsonRpcError;
use crate::utils::net::global_http_client;
use jsonrpsee::{
    core::{client::ClientT, traits::ToRpcParams},
    types::{Id, Request},
    ws_client::WsClientBuilder,
};
use serde::de::IntoDeserializer;
use serde::Deserialize;
use tracing::debug;

pub const API_INFO_KEY: &str = "FULLNODE_API_INFO";
pub const DEFAULT_HOST: &str = "127.0.0.1";
pub const DEFAULT_MULTIADDRESS: &str = "/ip4/127.0.0.1/tcp/2345/http";
pub const DEFAULT_PORT: u16 = 2345;
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Clone, Debug)]
pub struct ApiInfo {
    pub multiaddr: Multiaddr,
    pub token: Option<String>,
}

impl fmt::Display for ApiInfo {
    /// Convert an [`ApiInfo`] to a string
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
        Ok(match s.split_once(':') {
            // token:host
            Some((jwt, host)) => ApiInfo {
                multiaddr: host.parse()?,
                token: Some(jwt.to_owned()),
            },
            // host
            None => ApiInfo {
                multiaddr: s.parse()?,
                token: None,
            },
        })
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

    pub async fn call<T: HasLotusJson + std::fmt::Debug>(
        &self,
        req: RpcRequest<T>,
    ) -> Result<T, JsonRpcError> {
        let params = serde_json::value::to_raw_value(&req.params)
            .map_err(|e| JsonRpcError::invalid_params(e, None))?;
        let rpc_req = Request::new(req.method_name.into(), Some(&params), Id::Number(0));

        let api_url = multiaddress_to_url(
            &self.multiaddr,
            req.rpc_endpoint,
            CommunicationProtocol::Http,
        )
        .to_string();

        let request_log = format!(
            "JSON-RPC request URL: {}, payload: {}",
            api_url,
            serde_json::to_string(&rpc_req).unwrap_or_default()
        );
        debug!(request_log);

        let request = global_http_client()
            .post(api_url)
            .timeout(req.timeout)
            .json(&rpc_req);
        let request = match self.token.as_ref() {
            Some(token) => request.header(http0::header::AUTHORIZATION, token),
            _ => request,
        };

        let response = request.send().await?;
        let result = match response.status() {
            http0::StatusCode::NOT_FOUND => {
                Err(JsonRpcError::method_not_found("method_not_found", None))
            }
            http0::StatusCode::FORBIDDEN => Err(JsonRpcError::new(
                response.status().as_u16().into(),
                match &self.token {
                    Some(_) => "Permission denied: Insufficient rights.",
                    None => "Permission denied: Token required.",
                },
                None,
            )),
            other if !other.is_success() => Err(JsonRpcError::new(
                other.as_u16().into(),
                response.text().await?,
                None,
            )),
            _ok => {
                let bytes = response.bytes().await?;
                let response = serde_json::from_slice::<
                    jsonrpsee::types::Response<&serde_json::value::RawValue>,
                >(&bytes)
                .map_err(|e| JsonRpcError::parse_error(e, None))?;
                debug!(?response);
                match response.payload {
                    jsonrpsee::types::ResponsePayload::Success(it) => {
                        T::LotusJson::deserialize(it.into_deserializer())
                            .map(T::from_lotus_json)
                            .map_err(|e| JsonRpcError::parse_error(e, None))
                    }
                    jsonrpsee::types::ResponsePayload::Error(e) => {
                        Err(JsonRpcError::parse_error(e, None))
                    }
                }
            }
        };

        result
    }

    pub async fn ws_call<T: HasLotusJson + std::fmt::Debug + Send>(
        &self,
        req: RpcRequest<T>,
    ) -> Result<T, JsonRpcError> {
        let api_url =
            multiaddress_to_url(&self.multiaddr, req.rpc_endpoint, CommunicationProtocol::Ws);
        debug!("Using JSON-RPC v2 WS URL: {}", &api_url);
        let ws_client = WsClientBuilder::default()
            .request_timeout(req.timeout)
            .build(api_url.to_string())
            .await
            .map_err(|e| JsonRpcError::internal_error(e, None))?;
        let response = ws_client
            .request(req.method_name, req)
            .await
            .map(HasLotusJson::from_lotus_json)
            .map_err(|e| JsonRpcError::internal_error(e, None))?;
        debug!(?response);
        Ok(response)
    }
}

impl From<reqwest::Error> for JsonRpcError {
    fn from(e: reqwest::Error) -> Self {
        Self::new(
            e.status().map(|it| it.as_u16()).unwrap_or_default().into(),
            e,
            None,
        )
    }
}

struct Url {
    protocol: String,
    port: u16,
    host: String,
    endpoint: String,
}

impl fmt::Display for Url {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}://{}:{}/{}",
            self.protocol, self.host, self.port, self.endpoint
        )
    }
}

#[derive(PartialEq, Eq, Debug, strum::EnumString, strum::Display)]
pub enum CommunicationProtocol {
    #[strum(serialize = "http")]
    Http,
    #[strum(serialize = "ws")]
    Ws,
}

/// Parses a multi-address into a URL
fn multiaddress_to_url(
    multiaddr: &Multiaddr,
    endpoint: &str,
    protocol: CommunicationProtocol,
) -> Url {
    // Fold Multiaddress into a Url struct
    let addr = multiaddr.iter().fold(
        Url {
            protocol: protocol.to_string(),
            port: DEFAULT_PORT,
            host: DEFAULT_HOST.to_owned(),
            endpoint: endpoint.into(),
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
                Protocol::Ws(..) => {
                    addr.protocol = "ws".to_string();
                }
                Protocol::Wss(..) => {
                    addr.protocol = "wss".to_string();
                }
                _ => {}
            };
            addr
        },
    );

    addr
}

/// An `RpcRequest` is an at-rest description of a remote procedure call. It can
/// be invoked using `ApiInfo::call`.
///
/// When adding support for a new RPC method, the corresponding `RpcRequest`
/// value should be public for use in testing.
#[derive(Debug, Clone)]
pub struct RpcRequest<T = serde_json::Value> {
    pub method_name: &'static str,
    params: serde_json::Value,
    result_type: PhantomData<T>,
    rpc_endpoint: &'static str,
    timeout: Duration,
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
            rpc_endpoint: "rpc/v0",
            timeout: DEFAULT_TIMEOUT,
        }
    }

    pub fn new_v1<P: HasLotusJson>(method_name: &'static str, params: P) -> Self {
        RpcRequest {
            method_name,
            params: serde_json::to_value(HasLotusJson::into_lotus_json(params)).unwrap_or(
                serde_json::Value::String(
                    "INTERNAL ERROR: Parameters could not be serialized as JSON".to_string(),
                ),
            ),
            result_type: PhantomData,
            rpc_endpoint: "rpc/v1",
            timeout: DEFAULT_TIMEOUT,
        }
    }

    pub fn set_timeout(&mut self, timeout: Duration) {
        self.timeout = timeout;
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.set_timeout(timeout);
        self
    }

    // Discard type information about the response.
    pub fn lower(self) -> RpcRequest {
        RpcRequest {
            method_name: self.method_name,
            params: self.params,
            result_type: PhantomData,
            rpc_endpoint: self.rpc_endpoint,
            timeout: self.timeout,
        }
    }
}

impl<T> ToRpcParams for RpcRequest<T> {
    fn to_rpc_params(self) -> Result<Option<Box<serde_json::value::RawValue>>, serde_json::Error> {
        Ok(Some(serde_json::value::to_raw_value(&self.params)?))
    }
}
