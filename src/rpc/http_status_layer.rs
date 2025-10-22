// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! This HTTP middleware modifies the HTTP status code of responses
//! based on the [`http::StatusCode`] stored in the response extensions, set via the RPC middleware
//! [`crate::rpc::set_extension_layer::SetExtensionLayer`].

use futures::FutureExt as _;
use http::StatusCode;
use http::{Request, Response};
use std::{
    pin::Pin,
    task::{Context, Poll},
};
use tower::Layer;
use tower::Service;

#[derive(Clone, Default)]
pub(super) struct ModifyHttpStatusLayer {}

impl<S> Layer<S> for ModifyHttpStatusLayer {
    type Service = ModifyHttpStatus<S>;

    fn layer(&self, service: S) -> Self::Service {
        ModifyHttpStatus { service }
    }
}

#[derive(Debug, Clone)]
pub(super) struct ModifyHttpStatus<S> {
    pub service: S,
}

impl<S, ReqBody, ResBody> Service<Request<ReqBody>> for ModifyHttpStatus<S>
where
    S: Service<Request<ReqBody>, Response = Response<ResBody>>,
    S::Future: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    #[inline]
    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.service.poll_ready(cx)
    }

    fn call(&mut self, request: Request<ReqBody>) -> Self::Future {
        let fut = self.service.call(request);
        async move {
            let mut rp = fut.await?;
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
