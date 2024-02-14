// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use jsonrpsee::server::middleware::rpc::{ResponseFuture, RpcServiceT};
use jsonrpsee::{MethodResponse, Methods};

use tower::Layer;

#[derive(Clone)]
pub struct AuthLayer {
    pub headers: hyper::HeaderMap,
}

impl<S> Layer<S> for AuthLayer {
    type Service = AuthMiddleware<S>;

    fn layer(&self, service: S) -> Self::Service {
        AuthMiddleware {
            headers: self.headers.clone(),
            inner: service,
        }
    }
}

#[derive(Clone)]
pub struct AuthMiddleware<S> {
    pub headers: hyper::HeaderMap,
    pub inner: S,
}

impl<'a, S> RpcServiceT<'a> for AuthMiddleware<S>
where
    S: Send + Clone + Sync + RpcServiceT<'a>,
{
    type Future = ResponseFuture<S::Future>;

    fn call(&self, req: jsonrpsee::types::Request<'a>) -> Self::Future {
        tracing::debug!("{}", &req.method_name());

        tracing::debug!("{:?}", &self.headers.get(hyper::header::AUTHORIZATION));

        ResponseFuture::future(self.inner.call(req))
    }
}
