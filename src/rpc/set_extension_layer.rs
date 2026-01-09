// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::ApiPaths;
use jsonrpsee::MethodResponse;
use jsonrpsee::core::middleware::{Batch, BatchEntry, Notification};
use jsonrpsee::server::middleware::rpc::RpcServiceT;
use tower::Layer;

/// JSON-RPC middleware layer for setting extensions in RPC requests
#[derive(Clone)]
pub(super) struct SetExtensionLayer {
    pub path: ApiPaths,
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
    path: ApiPaths,
}

impl<S> RpcServiceT for SetExtensionService<S>
where
    S: RpcServiceT<MethodResponse = MethodResponse> + Send + Sync + Clone + 'static,
{
    type MethodResponse = S::MethodResponse;
    type NotificationResponse = S::NotificationResponse;
    type BatchResponse = S::BatchResponse;

    fn call<'a>(
        &self,
        mut req: jsonrpsee::types::Request<'a>,
    ) -> impl Future<Output = Self::MethodResponse> + Send + 'a {
        req.extensions_mut().insert(self.path);
        self.service.call(req)
    }

    fn batch<'a>(
        &self,
        mut batch: Batch<'a>,
    ) -> impl Future<Output = Self::BatchResponse> + Send + 'a {
        for req in batch.iter_mut() {
            match req {
                Ok(BatchEntry::Call(req)) => {
                    req.extensions_mut().insert(self.path);
                }
                Ok(BatchEntry::Notification(n)) => {
                    n.extensions_mut().insert(self.path);
                }
                Err(_) => {}
            }
        }
        self.service.batch(batch)
    }

    fn notification<'a>(
        &self,
        mut n: Notification<'a>,
    ) -> impl Future<Output = Self::NotificationResponse> + Send + 'a {
        n.extensions_mut().insert(self.path);
        self.service.notification(n)
    }
}
