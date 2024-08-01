// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::metrics;
use futures::future::BoxFuture;
use futures::FutureExt;
use jsonrpsee::server::middleware::rpc::RpcServiceT;
use jsonrpsee::types::{error::ErrorCode, ErrorObject};
use jsonrpsee::MethodResponse;
use tower::Layer;

#[derive(Clone)]
pub struct MetricsLayer {}

impl<S> Layer<S> for MetricsLayer {
    type Service = RecordMetrics<S>;

    fn layer(&self, service: S) -> Self::Service {
        RecordMetrics { service }
    }
}

#[derive(Clone)]
pub struct RecordMetrics<S> {
    service: S,
}

impl<'a, S> RpcServiceT<'a> for RecordMetrics<S>
where
    S: RpcServiceT<'a> + Send + Sync + Clone + 'static,
{
    type Future = BoxFuture<'a, MethodResponse>;

    fn call(&self, req: jsonrpsee::types::Request<'a>) -> Self::Future {
        let service = self.service.clone();

        metrics::RPC_METHOD_HIT
            .get_or_create(&metrics::RpcMethodLabel {
                method: req.method.to_string(),
            })
            .inc();

        async move { service.call(req).await }.boxed()
    }
}
