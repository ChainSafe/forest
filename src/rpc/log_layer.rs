// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Middleware layer for logging RPC calls.

use std::{
    borrow::Cow,
    fmt::Display,
    hash::{DefaultHasher, Hash as _, Hasher},
};

use futures::future::Either;
use jsonrpsee::MethodResponse;
use jsonrpsee::core::middleware::{Batch, Notification};
use jsonrpsee::{server::middleware::rpc::RpcServiceT, types::Id};
use tower::Layer;

// State-less jsonrpcsee layer for logging information about RPC calls
#[derive(Clone, Default)]
pub(super) struct LogLayer {}

impl<S> Layer<S> for LogLayer {
    type Service = Logging<S>;

    fn layer(&self, service: S) -> Self::Service {
        Logging { service }
    }
}

#[derive(Clone)]
pub(super) struct Logging<S> {
    service: S,
}

impl<S> Logging<S> {
    fn log_enabled() -> bool {
        tracing::enabled!(tracing::Level::DEBUG)
    }

    async fn log<F>(
        id: Id<'_>,
        method_name: impl Display,
        params: impl Display,
        future: F,
    ) -> MethodResponse
    where
        F: Future<Output = MethodResponse>,
    {
        // Avoid performance overhead if DEBUG level is not enabled.
        if !Self::log_enabled() {
            return future.await;
        }

        let start_time = std::time::Instant::now();
        let id = create_unique_id(id, start_time);
        tracing::trace!("RPC#{id}: {method_name}. Params: {params}");
        let resp = future.await;
        let elapsed = start_time.elapsed();
        let result = resp.as_error_code().map_or(Cow::Borrowed("OK"), |code| {
            Cow::Owned(format!("ERR({code})"))
        });
        tracing::debug!(
            "RPC#{id} {result}: {method_name}. Took {}",
            humantime::format_duration(elapsed)
        );
        resp
    }
}

impl<S> RpcServiceT for Logging<S>
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
        // Avoid performance overhead if DEBUG level is not enabled.
        if !Self::log_enabled() {
            Either::Left(self.service.call(req))
        } else {
            Either::Right(Self::log(
                req.id(),
                req.method_name().to_owned(),
                req.params().as_str().unwrap_or("[]").to_owned(),
                self.service.call(req),
            ))
        }
    }

    fn batch<'a>(&self, batch: Batch<'a>) -> impl Future<Output = Self::BatchResponse> + Send + 'a {
        self.service.batch(batch)
    }

    fn notification<'a>(
        &self,
        n: Notification<'a>,
    ) -> impl Future<Output = Self::NotificationResponse> + Send + 'a {
        // Avoid performance overhead if DEBUG level is not enabled.
        if !Self::log_enabled() {
            Either::Left(self.service.notification(n))
        } else {
            Either::Right(Self::log(
                Id::Null,
                n.method_name().to_owned(),
                "",
                self.service.notification(n),
            ))
        }
    }
}

/// Creates a unique ID for the RPC call, so it can be easily tracked in logs.
fn create_unique_id(id: Id, start_time: std::time::Instant) -> String {
    const ID_LEN: usize = 6;
    let mut hasher = DefaultHasher::new();
    id.hash(&mut hasher);
    start_time.hash(&mut hasher);
    let mut id = format!("{:x}", hasher.finish());
    id.truncate(ID_LEN);
    id
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn test_create_unique_id_same() {
        let id = Id::Number(1);
        let start_time = std::time::Instant::now();
        let id1 = create_unique_id(id.clone(), start_time);
        let id2 = create_unique_id(id, start_time);
        assert_eq!(id1, id2);
    }

    #[test]
    fn test_create_unique_id_different_ids() {
        let id1 = Id::Number(1);
        let id2 = Id::Number(2);
        let start_time = std::time::Instant::now();
        let id1 = create_unique_id(id1, start_time);
        let id2 = create_unique_id(id2, start_time);
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_create_unique_id_different_times() {
        let id = Id::Number(1);
        let start_time1 = std::time::Instant::now();
        let start_time2 = std::time::Instant::now() + Duration::from_nanos(1);
        let id1 = create_unique_id(id.clone(), start_time1);
        let id2 = create_unique_id(id, start_time2);
        assert_ne!(id1, id2);
    }
}
