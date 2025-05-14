// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::ApiPaths;
use futures::{FutureExt, future::BoxFuture};
use jsonrpsee::MethodResponse;
use jsonrpsee::server::middleware::rpc::RpcServiceT;
use tower::Layer;

/// JSON-RPC middleware layer for setting extensions in RPC requests
#[derive(Clone, Default)]
pub(super) struct SetExtensionLayer {
    pub path: Option<ApiPaths>,
}

impl<S> Layer<S> for SetExtensionLayer {
    type Service = SetExtensionService<S>;

    fn layer(&self, service: S) -> Self::Service {
        SetExtensionService {
            service,
            path: self.path,
        }
    }
}

#[derive(Clone)]
pub(super) struct SetExtensionService<S> {
    service: S,
    path: Option<ApiPaths>,
}

impl<'a, S> RpcServiceT<'a> for SetExtensionService<S>
where
    S: RpcServiceT<'a> + Send + Sync + Clone + 'static,
{
    type Future = BoxFuture<'a, MethodResponse>;

    fn call(&self, mut req: jsonrpsee::types::Request<'a>) -> Self::Future {
        req.extensions_mut()
            .insert(self.path.unwrap_or(ApiPaths::V1));
        self.service.call(req).boxed()
    }
}
