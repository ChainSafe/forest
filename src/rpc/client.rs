// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! # Design Goals
//! - use [`jsonrpsee`] clients and primitives.
//! - Support [`rpc::Request`](crate::rpc::Request).
//! - Support different
//!   - endpoint paths (`v0`, `v1`).
//!   - communication protocols (`ws`, `http`).
//! - Support per-request timeouts.

use std::env;
use std::fmt::{self, Debug};
use std::sync::LazyLock;
use std::time::Duration;

use anyhow::bail;
use futures::future::Either;
use http::{HeaderMap, HeaderValue, header};
use jsonrpsee::core::ClientError;
use jsonrpsee::core::client::ClientT as _;
use jsonrpsee::core::params::{ArrayParams, ObjectParams};
use jsonrpsee::core::traits::ToRpcParams;
use serde::de::DeserializeOwned;
use tracing::{Instrument, Level, debug};
use url::Url;

use super::{ApiPaths, MAX_REQUEST_BODY_SIZE, MAX_RESPONSE_BODY_SIZE, Request};

/// A JSON-RPC client that can dispatch either a [`crate::rpc::Request`] to a single URL.
pub struct Client {
    /// SHOULD end in a slash, due to our use of [`Url::join`].
    base_url: Url,
    token: Option<String>,
    // just having these versions inline is easier than using a map
    v0: tokio::sync::OnceCell<UrlClient>,
    v1: tokio::sync::OnceCell<UrlClient>,
    v2: tokio::sync::OnceCell<UrlClient>,
}

impl Client {
    /// Use either the URL in the environment or a default.
    ///
    /// If `token` is provided, use that over the token in either of the above.
    pub fn default_or_from_env(token: Option<&str>) -> anyhow::Result<Self> {
        static DEFAULT: LazyLock<Url> = LazyLock::new(|| "http://127.0.0.1:2345/".parse().unwrap());

        let mut base_url = match env::var("FULLNODE_API_INFO") {
            Ok(it) => {
                let crate::utils::UrlFromMultiAddr(url) = it.parse()?;
                url
            }
            Err(env::VarError::NotPresent) => DEFAULT.clone(),
            Err(e @ env::VarError::NotUnicode(_)) => bail!(e),
        };
        if token.is_some() && base_url.set_password(token).is_err() {
            bail!("couldn't set override password")
        }
        // Set default token if not provided
        if token.is_none() && base_url.password().is_none() {
            let client_config = crate::cli_shared::cli::Client::default();
            let default_token_path = client_config.default_rpc_token_path();
            if default_token_path.is_file() {
                if let Ok(token) = std::fs::read_to_string(&default_token_path) {
                    if base_url.set_password(Some(token.trim())).is_ok() {
                        tracing::debug!("Loaded the default RPC token");
                    } else {
                        tracing::warn!("Failed to set the default RPC token");
                    }
                } else {
                    tracing::warn!("Failed to load the default token file");
                }
            }
        }
        Ok(Self::from_url(base_url))
    }
    pub fn from_url(mut base_url: Url) -> Self {
        let token = base_url.password().map(Into::into);
        let _defer = base_url.set_password(None);
        Self {
            token,
            base_url,
            v0: Default::default(),
            v1: Default::default(),
            v2: Default::default(),
        }
    }
    pub fn base_url(&self) -> &Url {
        &self.base_url
    }
    pub async fn call<T: crate::lotus_json::HasLotusJson + std::fmt::Debug>(
        &self,
        req: Request<T>,
    ) -> Result<T, ClientError> {
        let max_api_path = req
            .api_path()
            .map_err(|e| ClientError::Custom(e.to_string()))?;
        let Request {
            method_name,
            params,
            timeout,
            ..
        } = req;
        let method_name = method_name.as_ref();
        let client = self.get_or_init_client(max_api_path).await?;
        let span = tracing::debug_span!("request", method = %method_name, url = %client.url);
        let work = async {
            // jsonrpsee's clients have a global `timeout`, but not a per-request timeout, which
            // RpcRequest expects.
            // So shim in our own timeout
            let result_or_timeout = tokio::time::timeout(
                timeout,
                match params {
                    serde_json::Value::Null => Either::Left(Either::Left(
                        client.request::<T::LotusJson, _>(method_name, ArrayParams::new()),
                    )),
                    serde_json::Value::Array(it) => {
                        let mut params = ArrayParams::new();
                        for param in it {
                            params.insert(param)?
                        }
                        trace_params(params.clone());
                        Either::Left(Either::Right(client.request(method_name, params)))
                    }
                    serde_json::Value::Object(it) => {
                        let mut params = ObjectParams::new();
                        for (name, param) in it {
                            params.insert(&name, param)?
                        }
                        trace_params(params.clone());
                        Either::Right(client.request(method_name, params))
                    }
                    prim @ (serde_json::Value::Bool(_)
                    | serde_json::Value::Number(_)
                    | serde_json::Value::String(_)) => {
                        return Err(ClientError::Custom(format!(
                            "invalid parameter type: `{prim}`"
                        )));
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
    async fn get_or_init_client(&self, path: ApiPaths) -> Result<&UrlClient, ClientError> {
        match path {
            ApiPaths::V0 => &self.v0,
            ApiPaths::V1 => &self.v1,
            ApiPaths::V2 => &self.v2,
        }
        .get_or_try_init(|| async {
            let url = self.base_url.join(path.path()).map_err(|it| {
                ClientError::Custom(format!("creating url for endpoint failed: {it}"))
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
    inner: UrlClientInner,
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
        const ONE_DAY: Duration = Duration::from_secs(24 * 3600); // we handle timeouts ourselves.
        let headers = match token.into() {
            Some(token) => HeaderMap::from_iter([(
                header::AUTHORIZATION,
                match HeaderValue::try_from(format!("Bearer {token}")) {
                    Ok(token) => token,
                    Err(e) => {
                        return Err(ClientError::Custom(format!(
                            "Invalid authorization token: {e}",
                        )));
                    }
                },
            )]),
            None => HeaderMap::new(),
        };
        let inner = match url.scheme() {
            "ws" | "wss" => UrlClientInner::Ws(
                jsonrpsee::ws_client::WsClientBuilder::new()
                    .set_headers(headers)
                    .max_request_size(MAX_REQUEST_BODY_SIZE)
                    .max_response_size(MAX_RESPONSE_BODY_SIZE)
                    .request_timeout(ONE_DAY)
                    .build(&url)
                    .await?,
            ),
            "http" | "https" => UrlClientInner::Https(
                jsonrpsee::http_client::HttpClientBuilder::new()
                    .set_headers(headers)
                    .max_request_size(MAX_REQUEST_BODY_SIZE)
                    .max_response_size(MAX_RESPONSE_BODY_SIZE)
                    .request_timeout(ONE_DAY)
                    .build(&url)?,
            ),
            it => {
                return Err(ClientError::Custom(format!("Unsupported URL scheme: {it}")));
            }
        };
        Ok(Self { url, inner })
    }
}

#[allow(clippy::large_enum_variant)]
enum UrlClientInner {
    Ws(jsonrpsee::ws_client::WsClient),
    Https(jsonrpsee::http_client::HttpClient),
}

impl jsonrpsee::core::client::ClientT for UrlClient {
    fn notification<Params>(
        &self,
        method: &str,
        params: Params,
    ) -> impl Future<Output = Result<(), jsonrpsee::core::client::Error>> + Send
    where
        Params: ToRpcParams + Send,
    {
        match &self.inner {
            UrlClientInner::Ws(it) => Either::Left(it.notification(method, params)),
            UrlClientInner::Https(it) => Either::Right(it.notification(method, params)),
        }
    }

    fn request<R, Params>(
        &self,
        method: &str,
        params: Params,
    ) -> impl Future<Output = Result<R, jsonrpsee::core::client::Error>> + Send
    where
        R: DeserializeOwned,
        Params: ToRpcParams + Send,
    {
        match &self.inner {
            UrlClientInner::Ws(it) => Either::Left(it.request(method, params)),
            UrlClientInner::Https(it) => Either::Right(it.request(method, params)),
        }
    }

    fn batch_request<'a, R>(
        &self,
        batch: jsonrpsee::core::params::BatchRequestBuilder<'a>,
    ) -> impl Future<
        Output = Result<
            jsonrpsee::core::client::BatchResponse<'a, R>,
            jsonrpsee::core::client::Error,
        >,
    > + Send
    where
        R: DeserializeOwned + fmt::Debug + 'a,
    {
        match &self.inner {
            UrlClientInner::Ws(it) => Either::Left(it.batch_request(batch)),
            UrlClientInner::Https(it) => Either::Right(it.batch_request(batch)),
        }
    }
}

impl jsonrpsee::core::client::SubscriptionClientT for UrlClient {
    fn subscribe<'a, N, Params>(
        &self,
        subscribe_method: &'a str,
        params: Params,
        unsubscribe_method: &'a str,
    ) -> impl Future<
        Output = Result<jsonrpsee::core::client::Subscription<N>, jsonrpsee::core::client::Error>,
    >
    where
        Params: ToRpcParams + Send,
        N: DeserializeOwned,
    {
        match &self.inner {
            UrlClientInner::Ws(it) => {
                Either::Left(it.subscribe(subscribe_method, params, unsubscribe_method))
            }
            UrlClientInner::Https(it) => {
                Either::Right(it.subscribe(subscribe_method, params, unsubscribe_method))
            }
        }
    }

    fn subscribe_to_method<N>(
        &self,
        method: &str,
    ) -> impl Future<
        Output = Result<jsonrpsee::core::client::Subscription<N>, jsonrpsee::core::client::Error>,
    >
    where
        N: DeserializeOwned,
    {
        match &self.inner {
            UrlClientInner::Ws(it) => Either::Left(it.subscribe_to_method(method)),
            UrlClientInner::Https(it) => Either::Right(it.subscribe_to_method(method)),
        }
    }
}
