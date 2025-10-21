// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! This HTTP middleware modifies the HTTP status code of responses
//! based on the StatusCode stored in the response extensions, set via the RPC middleware
//! [`crate::rpc::set_extension_layer::SetExtensionLayer`].

use axum::body::HttpBody as HttpBodyTrait;
use bytes::Bytes;
use futures::FutureExt as _;
use http::StatusCode;
use jsonrpsee::{
    core::BoxError,
    server::{HttpBody, HttpRequest, HttpResponse},
};
use std::{
    pin::Pin,
    task::{Context, Poll},
};
use tower::Service;
use tower_http::compression::CompressionBody;

#[derive(Debug, Clone)]
pub struct ModifyHttpStatus<S> {
    pub service: S,
}

impl<S, B> Service<HttpRequest<B>> for ModifyHttpStatus<S>
where
    S: Service<HttpRequest<B>, Response = HttpResponse<CompressionBody<HttpBody>>>,
    S::Response: 'static,
    S::Error: Into<BoxError> + Send + 'static,
    S::Future: Send + 'static,
    B: HttpBodyTrait<Data = Bytes> + Send + std::fmt::Debug + 'static,
    B::Data: Send,
    B::Error: Into<BoxError>,
{
    type Response = S::Response;
    type Error = BoxError;
    type Future =
        Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send + 'static>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.service.poll_ready(cx).map_err(Into::into)
    }

    fn call(&mut self, request: HttpRequest<B>) -> Self::Future {
        let fut = self.service.call(request);
        async move {
            let mut rp = fut.await.map_err(Into::into)?;
            let status_code = rp
                .extensions()
                .get::<StatusCode>()
                .copied()
                .unwrap_or_default();

            *rp.status_mut() = status_code;

            Ok(rp)
        }
        .boxed()
    }
}
