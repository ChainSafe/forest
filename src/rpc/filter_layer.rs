// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::sync::Arc;

use futures::future::BoxFuture;
use futures::FutureExt;
use jsonrpsee::server::middleware::rpc::RpcServiceT;
use jsonrpsee::types::ErrorObject;
use jsonrpsee::MethodResponse;
use tower::Layer;

use super::FilterList;

/// JSON-RPC middleware layer for filtering RPC methods based on their name.
#[derive(Clone, Default)]
pub(super) struct FilterLayer {
    filter_list: Arc<FilterList>,
}

impl FilterLayer {
    pub fn new(filter_list: FilterList) -> Self {
        Self {
            filter_list: Arc::new(filter_list),
        }
    }
}

impl<S> Layer<S> for FilterLayer {
    type Service = Filtering<S>;

    fn layer(&self, service: S) -> Self::Service {
        Filtering {
            service,
            filter_list: self.filter_list.clone(),
        }
    }
}

#[derive(Clone)]
pub(super) struct Filtering<S> {
    service: S,
    filter_list: Arc<FilterList>,
}

impl<'a, S> RpcServiceT<'a> for Filtering<S>
where
    S: RpcServiceT<'a> + Send + Sync + Clone + 'static,
{
    type Future = BoxFuture<'a, MethodResponse>;

    fn call(&self, req: jsonrpsee::types::Request<'a>) -> Self::Future {
        let service = self.service.clone();
        let authorized = self.filter_list.authorize(req.method_name());
        async move {
            if authorized {
                service.call(req).await
            } else {
                MethodResponse::error(
                    req.id(),
                    ErrorObject::borrowed(
                        http::StatusCode::FORBIDDEN.as_u16() as _,
                        "Forbidden",
                        None,
                    ),
                )
            }
        }
        .boxed()
    }
}
