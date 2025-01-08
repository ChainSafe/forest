// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Middleware layer for logging RPC calls.

use std::{
    borrow::Cow,
    hash::{DefaultHasher, Hash as _, Hasher},
};

use futures::future::BoxFuture;
use futures::FutureExt;
use jsonrpsee::MethodResponse;
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

impl<'a, S> RpcServiceT<'a> for Logging<S>
where
    S: RpcServiceT<'a> + Send + Sync + Clone + 'static,
{
    type Future = BoxFuture<'a, MethodResponse>;

    fn call(&self, req: jsonrpsee::types::Request<'a>) -> Self::Future {
        let service = self.service.clone();

        async move {
            // Avoid performance overhead if DEBUG level is not enabled.
            if !tracing::enabled!(tracing::Level::DEBUG) {
                return service.call(req).await;
            }

            let start_time = std::time::Instant::now();
            let method_name = req.method_name().to_owned();
            let id = req.id();
            let id = create_unique_id(id, start_time);

            tracing::trace!(
                "RPC#{id}: {method_name}. Params: {params}",
                params = req.params().as_str().unwrap_or("[]")
            );

            let resp = service.call(req).await;

            let elapsed = start_time.elapsed();
            let result = resp.as_error_code().map_or(Cow::Borrowed("OK"), |code| {
                Cow::Owned(format!("ERR({code})"))
            });
            tracing::debug!("RPC#{id} {result}: {method_name}. Took {elapsed:?}");

            resp
        }
        .boxed()
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
