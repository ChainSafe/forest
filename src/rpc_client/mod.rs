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

use std::borrow::Cow;
use std::env;
use std::fmt;
use std::marker::PhantomData;
use std::str::FromStr;
use std::time::Duration;

use crate::libp2p::{Multiaddr, Protocol};
use crate::lotus_json::HasLotusJson;
use crate::utils::net::global_http_client;
use base64::prelude::{Engine, BASE64_STANDARD};
use jsonrpc_v2::{Id, RequestObject, V2};
use serde::Deserialize;
use tracing::debug;

use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite;
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message as WsMessage};

pub const API_INFO_KEY: &str = "FULLNODE_API_INFO";
pub const DEFAULT_HOST: &str = "127.0.0.1";
pub const DEFAULT_MULTIADDRESS: &str = "/ip4/127.0.0.1/tcp/2345/http";
pub const DEFAULT_PORT: u16 = 2345;
pub const HTTP_PROTOCOL: &str = "http";
pub const WS_PROTOCOL: &str = "ws";
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
        let rpc_req = RequestObject::request()
            .with_method(req.method_name)
            .with_params(req.params)
            .with_id(0)
            .finish();

        let api_url =
            multiaddress_to_url(&self.multiaddr, req.rpc_endpoint, ComProtocol::Http).to_string();

        debug!("Using JSON-RPC v2 HTTP URL: {}", api_url);

        let request = global_http_client()
            .post(api_url)
            .timeout(req.timeout)
            .json(&rpc_req);
        let request = match self.token.as_ref() {
            Some(token) => request.header(http0::header::AUTHORIZATION, token),
            _ => request,
        };

        let response = request.send().await?;
        if response.status() == http0::StatusCode::NOT_FOUND {
            return Err(JsonRpcError::METHOD_NOT_FOUND);
        }
        if response.status() == http0::StatusCode::FORBIDDEN {
            let msg = if self.token.is_none() {
                "Permission denied: Token required."
            } else {
                "Permission denied: Insufficient rights."
            };
            return Err(JsonRpcError {
                code: response.status().as_u16() as i64,
                message: Cow::Borrowed(msg),
            });
        }
        if !response.status().is_success() {
            return Err(JsonRpcError {
                code: response.status().as_u16() as i64,
                message: Cow::Owned(response.text().await?),
            });
        }
        let rpc_res: JsonRpcResponse<T::LotusJson> = response.json().await?;

        let resp = match rpc_res {
            JsonRpcResponse::Result { result, .. } => Ok(HasLotusJson::from_lotus_json(result)),
            JsonRpcResponse::Error { error, .. } => Err(error),
        };

        tracing::debug!("Response: {:?}", resp);
        resp
    }

    pub async fn ws_call<T: HasLotusJson + std::fmt::Debug>(
        &self,
        req: RpcRequest<T>,
    ) -> Result<T, JsonRpcError> {
        let rpc_req = RequestObject::request()
            .with_method(req.method_name)
            .with_params(req.params)
            .with_id(0)
            .finish();

        let payload = serde_json::to_vec(&rpc_req).map_err(|_| JsonRpcError::INVALID_REQUEST)?;

        let api_url = multiaddress_to_url(&self.multiaddr, req.rpc_endpoint, ComProtocol::Ws);

        debug!("Using JSON-RPC v2 WS URL: {}", &api_url);

        // A 16 byte key (base64 encoded) is expected for `Sec-WebSocket-Key` during a websocket handshake
        // See 5. in https://datatracker.ietf.org/doc/html/rfc6455#section-4.2.1
        let key = BASE64_STANDARD.encode(b"TheGreatOldOnes.");

        let request = tungstenite::http::Request::builder()
            .method("GET")
            .uri(api_url.to_string())
            .header("Host", api_url.host)
            .header("Upgrade", "websocket")
            .header("Connection", "upgrade")
            .header("Sec-Websocket-Key", key)
            .header("Sec-Websocket-Version", "13")
            .body(())
            .map_err(|_| JsonRpcError::INVALID_REQUEST)?;

        let (ws_stream, _) = connect_async(request).await?;

        let (mut write, mut read) = ws_stream.split();

        write.send(WsMessage::Binary(payload)).await?;

        let resp = match tokio::time::timeout(req.timeout, read.next()).await {
            Ok(v) => {
                if let Some(message) = v {
                    let data = message?.into_data();
                    let rpc_res: JsonRpcResponse<T::LotusJson> =
                        serde_json::from_slice(&data).map_err(|_| JsonRpcError::PARSE_ERROR)?;

                    match rpc_res {
                        JsonRpcResponse::Result { result, .. } => {
                            Ok(HasLotusJson::from_lotus_json(result))
                        }
                        JsonRpcResponse::Error { error, .. } => Err(error),
                    }
                } else {
                    Err(JsonRpcError::INVALID_REQUEST)
                }
            }
            Err(_) => Err(JsonRpcError::TIMED_OUT),
        };

        tracing::debug!("Response: {:?}", resp);
        resp
    }
}

/// Error object in a response
#[derive(Debug, Deserialize, PartialEq, Eq)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: Cow<'static, str>,
}

impl JsonRpcError {
    // https://www.jsonrpc.org/specification#error_object
    // -32700 	Parse error 	Invalid JSON was received by the server.
    //                          An error occurred on the server while parsing the JSON text.
    // -32600 	Invalid Request 	The JSON sent is not a valid Request object.
    // -32601 	Method not found 	The method does not exist / is not available.
    // -32602 	Invalid params 	Invalid method parameter(s).
    // -32603 	Internal error 	Internal JSON-RPC error.
    // -32000 to -32099 	Server error 	Reserved for implementation-defined server-errors.
    pub const PARSE_ERROR: JsonRpcError = JsonRpcError {
        code: -32700,
        message: Cow::Borrowed(
            "Invalid JSON was received by the server. \
             An error occurred on the server while parsing the JSON text.",
        ),
    };
    pub const INVALID_REQUEST: JsonRpcError = JsonRpcError {
        code: -32600,
        message: Cow::Borrowed("The JSON sent is not a valid Request object."),
    };
    pub const METHOD_NOT_FOUND: JsonRpcError = JsonRpcError {
        code: -32601,
        message: Cow::Borrowed("The method does not exist / is not available."),
    };
    pub const INVALID_PARAMS: JsonRpcError = JsonRpcError {
        code: -32602,
        message: Cow::Borrowed("Invalid method parameter(s)."),
    };
    pub const TIMED_OUT: JsonRpcError = JsonRpcError {
        code: 0,
        message: Cow::Borrowed("Operation timed out."),
    };
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

impl From<tungstenite::Error> for JsonRpcError {
    fn from(tungstenite_error: tungstenite::Error) -> Self {
        let status = match &tungstenite_error {
            tungstenite::Error::Http(resp) => Some(resp.status()),
            _ => None,
        };
        JsonRpcError {
            code: status.map(|s| s.as_u16()).unwrap_or_default() as i64,
            message: Cow::Owned(tungstenite_error.to_string()),
        }
    }
}

impl From<reqwest::Error> for JsonRpcError {
    fn from(reqwest_error: reqwest::Error) -> Self {
        JsonRpcError {
            code: reqwest_error
                .status()
                .map(|s| s.as_u16())
                .unwrap_or_default() as i64,
            message: Cow::Owned(reqwest_error.to_string()),
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

// The communication protocol
enum ComProtocol {
    Http,
    Ws,
}

/// Parses a multi-address into a URL
fn multiaddress_to_url(multiaddr: &Multiaddr, endpoint: &str, com_protocol: ComProtocol) -> Url {
    // Fold Multiaddress into a Url struct
    let addr = multiaddr.iter().fold(
        Url {
            protocol: match com_protocol {
                ComProtocol::Http => HTTP_PROTOCOL,
                ComProtocol::Ws => WS_PROTOCOL,
            }
            .to_owned(),
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
