//! This module aims to support making JSON-RPC calls.
//!
//! # Design Goals
//! - use [`jsonrpsee`] clients and primitives.
//! - Support different call formats
//!   - [`crate::rpc_client::RpcRequest`]
//!   - [`crate::rpc::RpcMethod`]
//! - Support different
//!   - endpoint paths ("v0", "v1").
//!   - communication protocols ("ws", "http").
//! - Pool appropriately, making it suitable as a global client.
//! - Support per-request timeouts.

use std::fmt::{self, Debug, Display};
use std::sync::Arc;
use std::time::Duration;

use http0::{header, HeaderMap, HeaderValue};
use jsonrpsee::core::params::{ArrayParams, ObjectParams};
use jsonrpsee::core::ClientError;
use libp2p::multiaddr::Protocol;
use libp2p::Multiaddr;
use serde::de::DeserializeOwned;
use tracing::debug;
use url::Url;

pub struct Client {
    base_url: Url,
    clients: Arc<cachemap2::CacheMap<Url, OneClient>>,
}

/// Represents a single, persistent connection to a url over which requests can
/// be made.
struct OneClient {
    url: Url,
    inner: OneClientInner,
}

impl Debug for OneClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OneClient")
            .field("url", &self.url)
            .finish_non_exhaustive()
    }
}

impl OneClient {
    async fn from_multiaddr_with_path(
        multiaddr: &Multiaddr,
        path: impl Display,
        token: impl Into<Option<String>>,
    ) -> Result<Self, ClientError> {
        let Some(mut it) = multiaddr2url(&multiaddr) else {
            return Err(ClientError::Custom(format!(
                "Couldn't convert multiaddr `{}` to URL",
                multiaddr
            )));
        };
        it.set_path(&path.to_string());
        Self::from_url(it, token).await
    }
    async fn from_url(url: Url, token: impl Into<Option<String>>) -> Result<Self, ClientError> {
        let timeout = Duration::MAX; // we handle timeouts ourselves.
        let headers = match token.into() {
            Some(it) => HeaderMap::from_iter([(
                header::AUTHORIZATION,
                match HeaderValue::try_from(it) {
                    Ok(it) => it,
                    Err(e) => {
                        return Err(ClientError::Custom(format!(
                            "Invalid authorization token: {e}"
                        )))
                    }
                },
            )]),
            None => Default::default(),
        };
        let inner = match url.scheme() {
            "ws" | "wss" => OneClientInner::Ws(
                jsonrpsee::ws_client::WsClientBuilder::new()
                    .set_headers(headers)
                    .request_timeout(timeout)
                    .build(&url)
                    .await?,
            ),
            "http" | "https" => OneClientInner::Https(
                jsonrpsee::http_client::HttpClientBuilder::new()
                    .set_headers(headers)
                    .request_timeout(timeout)
                    .build(&url)?,
            ),
            it => return Err(ClientError::Custom(format!("Unsupported URL scheme: {it}"))),
        };
        Ok(Self { url, inner })
    }
}

enum OneClientInner {
    Ws(jsonrpsee::ws_client::WsClient),
    Https(jsonrpsee::http_client::HttpClient),
}

#[async_trait::async_trait]
impl jsonrpsee::core::client::ClientT for OneClient {
    async fn notification<P: jsonrpsee::core::traits::ToRpcParams + Send>(
        &self,
        method: &str,
        params: P,
    ) -> Result<(), jsonrpsee::core::ClientError> {
        match &self.inner {
            OneClientInner::Ws(it) => it.notification(method, params).await,
            OneClientInner::Https(it) => it.notification(method, params).await,
        }
    }
    async fn request<R: DeserializeOwned, P: jsonrpsee::core::traits::ToRpcParams + Send>(
        &self,
        method: &str,
        params: P,
    ) -> Result<R, jsonrpsee::core::ClientError> {
        match &self.inner {
            OneClientInner::Ws(it) => it.request(method, params).await,
            OneClientInner::Https(it) => it.request(method, params).await,
        }
    }
    async fn batch_request<'a, R: DeserializeOwned + 'a + std::fmt::Debug>(
        &self,
        batch: jsonrpsee::core::params::BatchRequestBuilder<'a>,
    ) -> Result<jsonrpsee::core::client::BatchResponse<'a, R>, jsonrpsee::core::ClientError> {
        match &self.inner {
            OneClientInner::Ws(it) => it.batch_request(batch).await,
            OneClientInner::Https(it) => it.batch_request(batch).await,
        }
    }
}

#[async_trait::async_trait]
impl jsonrpsee::core::client::SubscriptionClientT for OneClient {
    async fn subscribe<'a, Notif, Params>(
        &self,
        subscribe_method: &'a str,
        params: Params,
        unsubscribe_method: &'a str,
    ) -> Result<jsonrpsee::core::client::Subscription<Notif>, jsonrpsee::core::client::Error>
    where
        Params: jsonrpsee::core::traits::ToRpcParams + Send,
        Notif: DeserializeOwned,
    {
        match &self.inner {
            OneClientInner::Ws(it) => {
                it.subscribe(subscribe_method, params, unsubscribe_method)
                    .await
            }
            OneClientInner::Https(it) => {
                it.subscribe(subscribe_method, params, unsubscribe_method)
                    .await
            }
        }
    }
    async fn subscribe_to_method<'a, Notif>(
        &self,
        method: &'a str,
    ) -> Result<jsonrpsee::core::client::Subscription<Notif>, jsonrpsee::core::client::Error>
    where
        Notif: DeserializeOwned,
    {
        match &self.inner {
            OneClientInner::Ws(it) => it.subscribe_to_method(method).await,
            OneClientInner::Https(it) => it.subscribe_to_method(method).await,
        }
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
