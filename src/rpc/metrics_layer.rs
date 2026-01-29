// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::metrics;
use jsonrpsee::MethodResponse;
use jsonrpsee::core::middleware::{Batch, Notification};
use jsonrpsee::server::middleware::rpc::RpcServiceT;
use tower::Layer;

// State-less jsonrpcsee layer for measuring RPC metrics
#[derive(Clone, Default)]
pub(super) struct MetricsLayer {}

impl<S> Layer<S> for MetricsLayer {
    type Service = RecordMetrics<S>;

    fn layer(&self, service: S) -> Self::Service {
        RecordMetrics { service }
    }
}

#[derive(Clone)]
pub(super) struct RecordMetrics<S> {
    service: S,
}

impl<S> RecordMetrics<S> {
    async fn log<F>(method: String, future: F) -> MethodResponse
    where
        F: Future<Output = MethodResponse>,
    {
        let method = metrics::RpcMethodLabel { method };
        let start_time = std::time::Instant::now();
        let resp = future.await;
        metrics::RPC_METHOD_TIME
            .get_or_create(&method)
            // Observe the elapsed time in milliseconds
            .observe(start_time.elapsed().as_secs_f64() * 1000.0);
        if resp.is_error() {
            metrics::RPC_METHOD_FAILURE.get_or_create(&method).inc();
        }
        resp
    }
}

impl<S> RpcServiceT for RecordMetrics<S>
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
        Self::log(req.method_name().to_owned(), self.service.call(req))
    }

    fn batch<'a>(&self, batch: Batch<'a>) -> impl Future<Output = Self::BatchResponse> + Send + 'a {
        self.service.batch(batch)
    }

    fn notification<'a>(
        &self,
        n: Notification<'a>,
    ) -> impl Future<Output = Self::NotificationResponse> + Send + 'a {
        Self::log(n.method_name().to_owned(), self.service.notification(n))
    }
}
