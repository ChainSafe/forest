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

use crate::libp2p::{Multiaddr, Protocol};
use crate::lotus_json::HasLotusJson;
pub use crate::rpc::JsonRpcError;
use crate::rpc::{self, ApiVersion};
use jsonrpsee::{
    core::{client::ClientT, traits::ToRpcParams},
    ws_client::WsClientBuilder,
};
use std::{env, fmt, marker::PhantomData, str::FromStr, time::Duration};
use tracing::debug;
use url::Url;

pub const API_INFO_KEY: &str = "FULLNODE_API_INFO";
pub const DEFAULT_HOST: &str = "127.0.0.1";
pub const DEFAULT_MULTIADDRESS: &str = "/ip4/127.0.0.1/tcp/2345/http";
pub const DEFAULT_PORT: u16 = 2345;
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(60);

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

    // TODO(aatifsyed): https://github.com/ChainSafe/forest/issues/4032
    //                  This function should return jsonrpsee::core::ClientError,
    //                  but that change should wait until _after_ all the methods
    //                  have been migrated.
    pub async fn call<T: HasLotusJson + std::fmt::Debug>(
        &self,
        req: RpcRequest<T>,
    ) -> Result<T, JsonRpcError> {
        use jsonrpsee::core::ClientError;
        match rpc::Client::new(
            multiaddr2url(&self.multiaddr).ok_or(JsonRpcError::internal_error(
                "couldn't convert multiaddr to URL",
                None,
            ))?,
            self.token.clone(),
        )
        .call(req)
        .await
        {
            Ok(it) => Ok(it),
            Err(e) => match e {
                ClientError::Call(it) => Err(it.into()),
                other => Err(JsonRpcError::internal_error(other, None)),
            },
        }
    }

    // TODO(aatifsyed): https://github.com/ChainSafe/forest/issues/4032
    //                  This should not be a separate code path.
    pub async fn ws_call<T: HasLotusJson + std::fmt::Debug + Send>(
        &self,
        req: RpcRequest<T>,
    ) -> Result<T, JsonRpcError> {
        let api_url =
            multiaddress_to_url(&self.multiaddr, req.api_version, CommunicationProtocol::Ws);
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

struct UrlComponents {
    scheme: String,
    host: String,
    port: u16,
    path: String,
}

impl fmt::Display for UrlComponents {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self {
            scheme,
            host,
            port,
            path,
        } = self;
        write!(f, "{}://{}:{}/{}", scheme, host, port, path)
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
    api_version: ApiVersion,
    protocol: CommunicationProtocol,
) -> UrlComponents {
    let endpoint = match api_version {
        ApiVersion::V0 => "rpc/v0",
        ApiVersion::V1 => "rpc/v1",
    };
    // Fold Multiaddress into a Url struct
    let addr = multiaddr.iter().fold(
        UrlComponents {
            scheme: protocol.to_string(),
            port: DEFAULT_PORT,
            host: DEFAULT_HOST.to_owned(),
            path: endpoint.into(),
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
                    addr.scheme = "http".to_string();
                }
                Protocol::Https => {
                    addr.scheme = "https".to_string();
                }
                Protocol::Ws(..) => {
                    addr.scheme = "ws".to_string();
                }
                Protocol::Wss(..) => {
                    addr.scheme = "wss".to_string();
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
    pub params: serde_json::Value,
    pub result_type: PhantomData<T>,
    pub api_version: ApiVersion,
    pub timeout: Duration,
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
            api_version: ApiVersion::V0,
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
            api_version: ApiVersion::V1,
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
            api_version: self.api_version,
            timeout: self.timeout,
        }
    }
}

impl<T> ToRpcParams for RpcRequest<T> {
    fn to_rpc_params(self) -> Result<Option<Box<serde_json::value::RawValue>>, serde_json::Error> {
        Ok(Some(serde_json::value::to_raw_value(&self.params)?))
    }
}

/// `"/dns/example.com/tcp/8080/http" -> "http://example.com:8080/"`
///
/// Returns [`None`] on unsupported formats, or if there is a URL parsing error.
///
/// Note that [`Multiaddr`]s do NOT support a (URL) `path`, so that must be handled
/// out-of-band.
fn multiaddr2url(m: &Multiaddr) -> Option<Url> {
    let mut components = m.iter().peekable();
    let host = match components.next()? {
        Protocol::Dns(it) | Protocol::Dns4(it) | Protocol::Dns6(it) | Protocol::Dnsaddr(it) => {
            it.to_string()
        }
        Protocol::Ip4(it) => it.to_string(),
        Protocol::Ip6(it) => it.to_string(),
        _ => return None,
    };
    let port = components
        .next_if(|it| matches!(it, Protocol::Tcp(_)))
        .map(|it| match it {
            Protocol::Tcp(port) => port,
            _ => unreachable!(),
        });
    // ENHANCEMENT: could recognise `Tcp/443/Tls` as `https`
    let scheme = match components.next()? {
        Protocol::Http => "http",
        Protocol::Https => "https",
        Protocol::Ws(it) if it == "/" => "ws",
        Protocol::Wss(it) if it == "/" => "wss",
        _ => return None,
    };
    let None = components.next() else { return None };
    let parse_me = match port {
        Some(port) => format!("{}://{}:{}", scheme, host, port),
        None => format!("{}://{}", scheme, host),
    };
    parse_me.parse().ok()
}

#[test]
fn test_multiaddr2url() {
    #[track_caller]
    fn do_test(input: &str, expected: &str) {
        let multiaddr = input.parse().unwrap();
        let url = multiaddr2url(&multiaddr).unwrap();
        assert_eq!(url.as_str(), expected);
    }
    do_test("/dns/example.com/http", "http://example.com/");
    do_test("/dns/example.com/tcp/8080/http", "http://example.com:8080/");
    do_test("/ip4/127.0.0.1/wss", "wss://127.0.0.1/");
}
