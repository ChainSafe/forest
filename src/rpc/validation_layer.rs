// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use futures::future::Either;
use jsonrpsee::MethodResponse;
use jsonrpsee::core::middleware::{Batch, BatchEntry, BatchEntryErr, Notification};
use jsonrpsee::server::middleware::rpc::RpcServiceT;
use jsonrpsee::types::ErrorObject;
use tower::Layer;

use super::json_validator;

/// JSON-RPC error code for invalid request
const INVALID_REQUEST: i32 = -32600;

/// stateless jsonrpsee layer for validating JSON-RPC requests
#[derive(Clone, Default)]
pub(super) struct JsonValidationLayer;

impl<S> Layer<S> for JsonValidationLayer {
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

    fn validate_params(params_str: &str) -> Result<(), String> {
        json_validator::validate_json_for_duplicates(params_str)
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

        if let Err(e) =
            json_validator::validate_json_for_duplicates(req.params().as_str().unwrap_or("[]"))
        {
            let err = ErrorObject::owned(INVALID_REQUEST, e, None::<()>);
            return Either::Right(async move { MethodResponse::error(req.id(), err) });
        }

        Either::Left(self.service.call(req))
    }

    fn batch<'a>(
        &self,
        mut batch: Batch<'a>,
    ) -> impl Future<Output = Self::BatchResponse> + Send + 'a {
        let service = self.service.clone();

        async move {
            if !Self::validation_enabled() {
                return service.batch(batch).await;
            }

            for entry in batch.iter_mut() {
                if let Ok(batch_entry) = entry {
                    let params_str = match batch_entry.params() {
                        Some(p) => p.as_ref().get(),
                        None => continue,
                    };

                    if let Err(e) = Self::validate_params(params_str) {
                        match batch_entry {
                            BatchEntry::Call(req) => {
                                let err = ErrorObject::owned(INVALID_REQUEST, e, None::<()>);
                                let err_entry = BatchEntryErr::new(req.id().clone(), err);
                                *entry = Err(err_entry);
                            }
                            BatchEntry::Notification(_notif) => {
                                let err = ErrorObject::owned(INVALID_REQUEST, e, None::<()>);
                                let err_entry = BatchEntryErr::new(jsonrpsee::types::Id::Null, err);
                                *entry = Err(err_entry);
                            }
                        }
                    }
                }
            }

            service.batch(batch).await
        }
    }

    fn notification<'a>(
        &self,
        n: Notification<'a>,
    ) -> impl Future<Output = Self::NotificationResponse> + Send + 'a {
        let service = self.service.clone();

        async move {
            if !Self::validation_enabled() {
                return service.notification(n).await;
            }

            let params_str = match n.params() {
                Some(p) => p.as_ref().get(),
                None => return service.notification(n).await,
            };

            if let Err(e) = Self::validate_params(params_str) {
                let err = ErrorObject::owned(INVALID_REQUEST, e, None::<()>);
                return MethodResponse::error(jsonrpsee::types::Id::Null, err);
            }

            service.notification(n).await
        }
    }
}
