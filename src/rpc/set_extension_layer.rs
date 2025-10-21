// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::ApiPaths;
use http::StatusCode;
use jsonrpsee::MethodResponse;
use jsonrpsee::core::middleware::{Batch, Notification};
use jsonrpsee::server::middleware::rpc::RpcServiceT;
use jsonrpsee::types::error::{INVALID_REQUEST_CODE, METHOD_NOT_FOUND_CODE};
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

impl<S> SetExtensionService<S> {
    /// Maps JSON-RPC error codes to HTTP status codes via extensions.
    /// Note that an HTTP middleware is required to actually set the HTTP status code.
    /// See [`crate::rpc::http_status_layer::ModifyHttpStatus`].
    async fn map_json_code_to_http_status<F>(future: F) -> MethodResponse
    where
        F: Future<Output = MethodResponse>,
    {
        let mut resp = future.await;
        if let Some(error_code) = resp.as_error_code() {
            // mapping as per https://www.jsonrpc.org/historical/json-rpc-over-http.html#errors
            let status = match error_code {
                INVALID_REQUEST_CODE => StatusCode::BAD_REQUEST,
                METHOD_NOT_FOUND_CODE => StatusCode::NOT_FOUND,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            };
            resp.extensions_mut().insert(status);
            resp
        } else {
            resp
        }
    }
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
        req.extensions_mut()
            .insert(self.path.unwrap_or(ApiPaths::V1));
        Self::map_json_code_to_http_status(self.service.call(req))
    }

    fn batch<'a>(
        &self,
        mut batch: Batch<'a>,
    ) -> impl Future<Output = Self::BatchResponse> + Send + 'a {
        batch
            .extensions_mut()
            .insert(self.path.unwrap_or(ApiPaths::V1));
        self.service.batch(batch)
    }

    fn notification<'a>(
        &self,
        mut n: Notification<'a>,
    ) -> impl Future<Output = Self::NotificationResponse> + Send + 'a {
        n.extensions_mut().insert(self.path.unwrap_or(ApiPaths::V1));
        self.service.notification(n)
    }
}
