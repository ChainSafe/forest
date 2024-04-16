// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! # Design Goals
//! - use [`jsonrpsee`] clients and primitives.
//! - Support different call formats
//!   - [`crate::rpc_client::RpcRequest`]
//!   - [`crate::rpc::RpcMethod`]
//! - Support different
//!   - endpoint paths (`v0`, `v1`).
//!   - communication protocols (`ws`, `http`).
//! - Support per-request timeouts.

use std::fmt::{self, Debug};
use std::time::Duration;

use http02::{header, HeaderMap, HeaderValue};
use jsonrpsee::core::client::ClientT as _;
use jsonrpsee::core::params::{ArrayParams, ObjectParams};
use jsonrpsee::core::ClientError;
use serde::de::DeserializeOwned;
use tracing::{debug, Instrument, Level};
use url::Url;

use super::{ApiVersion, MAX_REQUEST_BODY_SIZE, MAX_RESPONSE_BODY_SIZE};

/// A JSON-RPC client that can dispatch either a [`crate::rpc_client::RpcRequest`]
/// or a [`crate::rpc::RpcMethod`] to a single URL.
pub struct Client {
    /// SHOULD end in a slash, due to our use of [`Url::join`].
    base_url: Url,
    token: Option<String>,
    // just having these versions inline is easier than using a map
    v0: tokio::sync::OnceCell<UrlClient>,
    v1: tokio::sync::OnceCell<UrlClient>,
}

impl Client {
    pub fn new(base_url: Url, token: impl Into<Option<String>>) -> Self {
        Self {
            base_url,
            token: token.into(),
            v0: Default::default(),
            v1: Default::default(),
        }
    }
    pub async fn call<T: crate::lotus_json::HasLotusJson + std::fmt::Debug>(
        &self,
        req: crate::rpc_client::RpcRequest<T>,
    ) -> Result<T, ClientError> {
        let crate::rpc_client::RpcRequest {
            method_name,
            params,
            api_version,
            timeout,
            ..
        } = req;

        let client = self.get_or_init_client(api_version).await?;
        let span = tracing::debug_span!("request", method = %method_name, url = %client.url);
        let work = async {
            // jsonrpsee's clients have a global `timeout`, but not a per-request timeout, which
            // RpcRequest expects.
            // So shim in our own timeout
            let result_or_timeout = tokio::time::timeout(
                timeout,
                match params {
                    serde_json::Value::Null => {
                        client.request::<T::LotusJson, _>(method_name, ArrayParams::new())
                    }
                    serde_json::Value::Array(it) => {
                        let mut params = ArrayParams::new();
                        for param in it {
                            params.insert(param)?
                        }
                        trace_params(params.clone());
                        client.request(method_name, params)
                    }
                    serde_json::Value::Object(it) => {
                        let mut params = ObjectParams::new();
                        for (name, param) in it {
                            params.insert(&name, param)?
                        }
                        trace_params(params.clone());
                        client.request(method_name, params)
                    }
                    prim @ (serde_json::Value::Bool(_)
                    | serde_json::Value::Number(_)
                    | serde_json::Value::String(_)) => {
                        return Err(ClientError::Custom(format!(
                            "invalid parameter type: `{}`",
                            prim
                        )))
                    }
                },
            )
            .await;
            let result = match result_or_timeout {
                Ok(Ok(it)) => Ok(T::from_lotus_json(it)),
                Ok(Err(e)) => Err(e),
                Err(_) => Err(ClientError::RequestTimeout),
            };
            debug!(?result);
            result
        };
        work.instrument(span.or_current()).await
    }
    async fn get_or_init_client(&self, version: ApiVersion) -> Result<&UrlClient, ClientError> {
        match version {
            ApiVersion::V0 => &self.v0,
            ApiVersion::V1 => &self.v1,
        }
        .get_or_try_init(|| async {
            let url = self
                .base_url
                .join(match version {
                    ApiVersion::V0 => "rpc/v0",
                    ApiVersion::V1 => "rpc/v1",
                })
                .map_err(|it| {
                    ClientError::Custom(format!("creating url for endpoint failed: {}", it))
                })?;
            UrlClient::new(url, self.token.clone()).await
        })
        .await
    }
}

fn trace_params(params: impl jsonrpsee::core::traits::ToRpcParams) {
    if tracing::enabled!(Level::TRACE) {
        match params.to_rpc_params() {
            Ok(Some(it)) => tracing::trace!(params = %it),
            Ok(None) => tracing::trace!("no params"),
            Err(error) => tracing::trace!(%error, "couldn't decode params"),
        }
    }
}

/// Represents a single, perhaps persistent connection to a URL over which requests
/// can be made using [`jsonrpsee`] primitives.
struct UrlClient {
    url: Url,
    inner: OneClientInner,
}

impl Debug for UrlClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OneClient")
            .field("url", &self.url)
            .finish_non_exhaustive()
    }
}

impl UrlClient {
    async fn new(url: Url, token: impl Into<Option<String>>) -> Result<Self, ClientError> {
        let timeout = Duration::MAX; // we handle timeouts ourselves.
        let headers = match token.into() {
            Some(it) => HeaderMap::from_iter([(
                header::AUTHORIZATION,
                match HeaderValue::try_from(it) {
                    Ok(it) => it,
                    Err(e) => {
                        return Err(ClientError::Custom(format!(
                            "Invalid authorization token: {}",
                            e
                        )))
                    }
                },
            )]),
            None => HeaderMap::new(),
        };
        let inner = match url.scheme() {
            "ws" | "wss" => OneClientInner::Ws(
                jsonrpsee::ws_client::WsClientBuilder::new()
                    .set_headers(headers)
                    .request_timeout(timeout)
                    .max_request_size(MAX_REQUEST_BODY_SIZE)
                    .max_response_size(MAX_RESPONSE_BODY_SIZE)
                    .build(&url)
                    .await?,
            ),
            "http" | "https" => OneClientInner::Https(
                jsonrpsee::http_client::HttpClientBuilder::new()
                    .set_headers(headers)
                    .max_request_size(MAX_REQUEST_BODY_SIZE)
                    .max_response_size(MAX_RESPONSE_BODY_SIZE)
                    .request_timeout(timeout)
                    .build(&url)?,
            ),
            it => {
                return Err(ClientError::Custom(format!(
                    "Unsupported URL scheme: {}",
                    it
                )))
            }
        };
        Ok(Self { url, inner })
    }
}

enum OneClientInner {
    Ws(jsonrpsee::ws_client::WsClient),
    Https(jsonrpsee::http_client::HttpClient),
}

#[async_trait::async_trait]
impl jsonrpsee::core::client::ClientT for UrlClient {
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
impl jsonrpsee::core::client::SubscriptionClientT for UrlClient {
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
