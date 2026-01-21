// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use futures::future::Either;
use jsonrpsee::server::middleware::rpc::RpcServiceT;
use jsonrpsee::types::ErrorObject;
use jsonrpsee::MethodResponse;
use tower::Layer;

use super::json_validator;

/// stateless jsonrpsee layer for validating JSON-RPC requests
#[derive(Clone, Default)]
pub(super) struct ValidationLayer;

impl<S> Layer<S> for ValidationLayer {
    type Service = Validation<S>;

    fn layer(&self, service: S) -> Self::Service {
        Validation { service }
    }
}

#[derive(Clone)]
pub(super) struct Validation<S> {
    service: S,
}

impl<S> Validation<S> {
    fn validation_enabled() -> bool {
        json_validator::is_strict_mode()
    }
}

impl<S> RpcServiceT for Validation<S>
where
    S: RpcServiceT<MethodResponse = MethodResponse, NotificationResponse = MethodResponse>
        + Send
        + Sync
        + Clone
        + 'static,
{
    type MethodResponse = S::MethodResponse;
    type NotificationResponse = S::NotificationResponse;
    type BatchResponse = S::BatchResponse;

    fn call<'a>(
        &self,
        req: jsonrpsee::types::Request<'a>,
    ) -> impl Future<Output = Self::MethodResponse> + Send + 'a {
        if !Self::validation_enabled() {
            return Either::Left(self.service.call(req));
        }

        if let Err(e) = json_validator::validate_json_for_duplicates(req.params().as_str().unwrap_or("[]")) {
            let err = ErrorObject::owned(
                -32600,
                e,
                None::<()>,
            );
            return Either::Right(async move { MethodResponse::error(req.id(), err) });
        }

        Either::Left(self.service.call(req))
    }

    fn batch<'a>(
        &self,
        batch: jsonrpsee::core::middleware::Batch<'a>,
    ) -> impl Future<Output = Self::BatchResponse> + Send + 'a {
        self.service.batch(batch)
    }

    fn notification<'a>(
        &self,
        n: jsonrpsee::core::middleware::Notification<'a>,
    ) -> impl Future<Output = Self::NotificationResponse> + Send + 'a {
        self.service.notification(n)
    }
}
