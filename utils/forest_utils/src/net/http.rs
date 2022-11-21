// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use async_trait::async_trait;
use hyper::{client::HttpConnector, Body};
use hyper_rustls::{HttpsConnector, HttpsConnectorBuilder};

/// Constructs [hyper::Client] that supports both `http` and `https`.
/// Note that only `http1` is supported.
pub fn https_client() -> hyper::Client<HttpsConnector<HttpConnector>> {
    hyper::Client::builder().build::<_, Body>(
        HttpsConnectorBuilder::new()
            .with_native_roots()
            .https_or_http()
            .enable_http1()
            .build(),
    )
}

/// Trait that contains extension methods of [Body]
#[async_trait]
pub trait HyperBodyExt
where
    Self: Sized,
{
    /// Converts [Body] into JSON
    async fn json<T>(self) -> anyhow::Result<T>
    where
        T: serde::de::DeserializeOwned;
}

#[async_trait]
impl HyperBodyExt for Body {
    async fn json<T>(self) -> anyhow::Result<T>
    where
        T: serde::de::DeserializeOwned,
    {
        let bytes = hyper::body::to_bytes(self).await?;
        Ok(serde_json::from_slice(&bytes)?)
    }
}
