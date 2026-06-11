// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::metrics;
use crate::{for_each_rpc_method, rpc::reflect::RpcMethod as _};
use ahash::HashMap;
use futures::future::Either;
use jsonrpsee::MethodResponse;
use jsonrpsee::core::middleware::{Batch, Notification};
use jsonrpsee::server::middleware::rpc::RpcServiceT;
use prometheus_client::metrics::{counter::Counter, histogram::Histogram};
use std::sync::LazyLock;
use tower::Layer;

/// Pre-resolved Prometheus handles for a single RPC method.
struct MethodMetrics {
    time: Histogram,
    failure: Counter,
}

/// Metric handles for every known RPC method, resolved once on first use.
static METHOD_METRICS: LazyLock<HashMap<&'static str, MethodMetrics>> = LazyLock::new(|| {
    fn register(map: &mut HashMap<&'static str, MethodMetrics>, name: &'static str) {
        let label = metrics::RpcMethodLabel { method: name };
        let time = metrics::RPC_METHOD_TIME.get_or_create(&label).clone();
        let failure = metrics::RPC_METHOD_FAILURE.get_or_create(&label).clone();
        map.insert(name, MethodMetrics { time, failure });
    }
    let mut map = HashMap::default();
    macro_rules! insert {
        ($ty:ty) => {
            register(&mut map, <$ty>::NAME);
            if let Some(alias) = <$ty>::NAME_ALIAS {
                register(&mut map, alias);
            }
        };
    }
    for_each_rpc_method!(insert);
    register(&mut map, crate::rpc::chain::CHAIN_NOTIFY);
    register(&mut map, crate::rpc::channel::CANCEL_METHOD_NAME);
    // Catch-all for any name that reaches this layer without its own entry
    // (unknown methods are normally rejected upstream by the segregation/auth
    // layers). Bounds label cardinality and avoids a per-call allocation.
    register(&mut map, "unknown");
    map
});

/// Look up the pre-resolved handles for `method`, falling back to the `"unknown"`
/// bucket for unrecognized names.
fn method_metrics(method: &str) -> &'static MethodMetrics {
    METHOD_METRICS
        .get(method)
        .or_else(|| METHOD_METRICS.get("unknown"))
        .expect("`unknown` metrics entry is always registered")
}

// `FOREST_RPC_METRICS_DISABLED` disables only the RPC metrics layer, leaving the
// metrics endpoint and all other metrics (cache, sync, database, ...) intact. To
// turn off metrics entirely, disable the endpoint instead (`--no-metrics`).
crate::def_is_env_truthy!(is_rpc_metrics_disabled, "FOREST_RPC_METRICS_DISABLED");

/// Whether the RPC metrics layer records per-call metrics.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MetricsMode {
    Enabled,
    Disabled,
}

impl From<bool> for MetricsMode {
    fn from(enabled: bool) -> Self {
        if enabled {
            Self::Enabled
        } else {
            Self::Disabled
        }
    }
}

/// State-less jsonrpsee layer for measuring RPC metrics.
#[derive(Clone)]
pub(super) struct MetricsLayer {
    mode: MetricsMode,
}

impl MetricsLayer {
    pub(super) fn new(mode: MetricsMode) -> Self {
        // `FOREST_RPC_METRICS_DISABLED` turns off RPC metrics specifically,
        // independent of whether the metrics endpoint (and other metrics) are on.
        let mode = if is_rpc_metrics_disabled() {
            MetricsMode::Disabled
        } else {
            mode
        };
        Self { mode }
    }
}

impl<S> Layer<S> for MetricsLayer {
    type Service = RecordMetrics<S>;

    fn layer(&self, service: S) -> Self::Service {
        RecordMetrics {
            service,
            mode: self.mode,
        }
    }
}

#[derive(Clone)]
pub(super) struct RecordMetrics<S> {
    service: S,
    mode: MetricsMode,
}

impl<S> RecordMetrics<S> {
    async fn log<F>(method: &'static MethodMetrics, future: F) -> MethodResponse
    where
        F: Future<Output = MethodResponse>,
    {
        let start_time = std::time::Instant::now();
        let resp = future.await;
        // Observe the elapsed time in milliseconds.
        method
            .time
            .observe(start_time.elapsed().as_secs_f64() * 1000.0);
        if resp.is_error() {
            method.failure.inc();
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
        if self.mode == MetricsMode::Enabled {
            Either::Right(Self::log(
                method_metrics(req.method_name()),
                self.service.call(req),
            ))
        } else {
            Either::Left(self.service.call(req))
        }
    }

    fn batch<'a>(&self, batch: Batch<'a>) -> impl Future<Output = Self::BatchResponse> + Send + 'a {
        self.service.batch(batch)
    }

    fn notification<'a>(
        &self,
        n: Notification<'a>,
    ) -> impl Future<Output = Self::NotificationResponse> + Send + 'a {
        if self.mode == MetricsMode::Enabled {
            Either::Right(Self::log(
                method_metrics(n.method_name()),
                self.service.notification(n),
            ))
        } else {
            Either::Left(self.service.notification(n))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_known_and_unknown_methods() {
        // Building `METHOD_METRICS` registers every RPC method without panicking
        // (e.g. no duplicate or missing names), and a known alias resolves to its
        // own pre-registered handle.
        let _ = method_metrics("eth_getBlockByNumber");
        // An unrecognized method falls back to the `"unknown"` bucket.
        let _ = method_metrics("Bogus.Nonexistent");
        assert!(METHOD_METRICS.contains_key("unknown"));
        assert!(METHOD_METRICS.contains_key(crate::rpc::chain::CHAIN_NOTIFY));
    }

    #[test]
    #[serial_test::serial]
    fn rpc_metrics_disabled_env_forces_passthrough() {
        // SAFETY: guarded by `#[serial]` so no other test races on the env var.
        unsafe { std::env::set_var("FOREST_RPC_METRICS_DISABLED", "1") };
        // Even when constructed `Enabled`, the env var forces the layer off.
        assert_eq!(
            MetricsLayer::new(MetricsMode::Enabled).mode,
            MetricsMode::Disabled
        );

        unsafe { std::env::remove_var("FOREST_RPC_METRICS_DISABLED") };
        assert_eq!(
            MetricsLayer::new(MetricsMode::Enabled).mode,
            MetricsMode::Enabled
        );
        assert_eq!(
            MetricsLayer::new(MetricsMode::Disabled).mode,
            MetricsMode::Disabled
        );
    }
}
