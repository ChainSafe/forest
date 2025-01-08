// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::metrics;
use futures::future::BoxFuture;
use futures::FutureExt;
use jsonrpsee::server::middleware::rpc::RpcServiceT;
use jsonrpsee::MethodResponse;
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

impl<'a, S> RpcServiceT<'a> for RecordMetrics<S>
where
    S: RpcServiceT<'a> + Send + Sync + Clone + 'static,
{
    type Future = BoxFuture<'a, MethodResponse>;

    fn call(&self, req: jsonrpsee::types::Request<'a>) -> Self::Future {
        let service = self.service.clone();
        let method = metrics::RpcMethodLabel {
            method: req.method_name().to_owned(),
        };

        async move {
            // Cannot use HistogramTimerExt::start_timer here since it would lock the metric.
            let start_time = std::time::Instant::now();
            let resp = service.call(req).await;

            metrics::RPC_METHOD_TIME
                .get_or_create(&method)
                // Observe the elapsed time in milliseconds
                .observe(start_time.elapsed().as_secs_f64() * 1000.0);

            if resp.is_error() {
                metrics::RPC_METHOD_FAILURE.get_or_create(&method).inc();
            }

            resp
        }
        .boxed()
    }
}
