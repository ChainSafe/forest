use std::fmt::Display;

use http0::{header, HeaderMap, HeaderValue};
use jsonrpsee::core::ClientError;
use libp2p::multiaddr::Protocol;
use libp2p::Multiaddr;
use serde::de::DeserializeOwned;
use url::Url;

pub struct Client {
    inner: ClientInner,
}

impl Client {
    pub async fn from_multiaddr_with_path<'a>(
        multiaddr: &Multiaddr,
        path: impl Display,
        token: impl Into<Option<&'a str>>,
    ) -> Result<Self, ClientError> {
        let Some(mut it) = multiaddr2url(&multiaddr) else {
            return Err(ClientError::Custom(String::from(
                "Couldn't convert multiaddr to URL",
            )));
        };
        it.set_path(&path.to_string());
        Self::from_url(&it, token).await
    }
    pub async fn from_url<'a>(
        url: &Url,
        token: impl Into<Option<&'a str>>,
    ) -> Result<Self, ClientError> {
        let headers = match token.into() {
            Some(it) => HeaderMap::from_iter([(
                header::AUTHORIZATION,
                match HeaderValue::from_str(it) {
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
            "ws" | "wss" => ClientInner::Ws(
                jsonrpsee::ws_client::WsClientBuilder::new()
                    .set_headers(headers)
                    .build(url)
                    .await
                    .unwrap(),
            ),
            "http" | "https" => ClientInner::Https(
                jsonrpsee::http_client::HttpClientBuilder::new()
                    .set_headers(headers)
                    .build(url)
                    .unwrap(),
            ),
            it => return Err(ClientError::Custom(format!("Unsupported URL scheme: {it}"))),
        };
        Ok(Self { inner })
    }
}

enum ClientInner {
    Ws(jsonrpsee::ws_client::WsClient),
    Https(jsonrpsee::http_client::HttpClient),
}

#[async_trait::async_trait]
impl jsonrpsee::core::client::ClientT for Client {
    async fn notification<P: jsonrpsee::core::traits::ToRpcParams + Send>(
        &self,
        method: &str,
        params: P,
    ) -> Result<(), jsonrpsee::core::ClientError> {
        match &self.inner {
            ClientInner::Ws(it) => it.notification(method, params).await,
            ClientInner::Https(it) => it.notification(method, params).await,
        }
    }
    async fn request<R: DeserializeOwned, P: jsonrpsee::core::traits::ToRpcParams + Send>(
        &self,
        method: &str,
        params: P,
    ) -> Result<R, jsonrpsee::core::ClientError> {
        match &self.inner {
            ClientInner::Ws(it) => it.request(method, params).await,
            ClientInner::Https(it) => it.request(method, params).await,
        }
    }
    async fn batch_request<'a, R: DeserializeOwned + 'a + std::fmt::Debug>(
        &self,
        batch: jsonrpsee::core::params::BatchRequestBuilder<'a>,
    ) -> Result<jsonrpsee::core::client::BatchResponse<'a, R>, jsonrpsee::core::ClientError> {
        match &self.inner {
            ClientInner::Ws(it) => it.batch_request(batch).await,
            ClientInner::Https(it) => it.batch_request(batch).await,
        }
    }
}

fn multiaddr2url(m: &Multiaddr) -> Option<Url> {
    let mut components = m.iter().peekable();
    let host = match components.next()? {
        Protocol::Dns4(it) | Protocol::Dns6(it) | Protocol::Dnsaddr(it) => it.to_string(),
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
