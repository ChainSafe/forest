// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::sync::Arc;

use jsonrpsee::MethodResponse;
use jsonrpsee::core::middleware::{Batch, Notification};
use jsonrpsee::server::middleware::rpc::RpcServiceT;
use jsonrpsee::types::ErrorObject;
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

impl<S> RpcServiceT for Filtering<S>
where
    S: RpcServiceT<MethodResponse = MethodResponse> + Send + Sync + Clone + 'static,
{
    type MethodResponse = S::MethodResponse;
    type NotificationResponse = S::NotificationResponse;
    type BatchResponse = S::BatchResponse;

    fn call<'a>(
        &self,
        req: jsonrpsee::types::Request<'a>,
    ) -> impl Future<Output = Self::MethodResponse> + Send + 'a {
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
    }

    fn batch<'a>(&self, batch: Batch<'a>) -> impl Future<Output = Self::BatchResponse> + Send + 'a {
        self.service.batch(batch)
    }

    fn notification<'a>(
        &self,
        n: Notification<'a>,
    ) -> impl Future<Output = Self::NotificationResponse> + Send + 'a {
        self.service.notification(n)
    }
}
