// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::ApiPaths;
use crate::{for_each_rpc_method, rpc::reflect::RpcMethod};
use ahash::{HashMap, HashSet};
use futures::future::Either;
use itertools::Itertools as _;
use jsonrpsee::MethodResponse;
use jsonrpsee::core::middleware::{Batch, BatchEntry, BatchEntryErr, Notification};
use jsonrpsee::server::middleware::rpc::RpcServiceT;
use jsonrpsee::types::{ErrorObject, Id};
use once_cell::sync::Lazy;
use tower::Layer;

static VERSION_METHODS_MAPPINGS: Lazy<HashMap<ApiPaths, HashSet<&'static str>>> = Lazy::new(|| {
    let mut map = HashMap::default();
    for version in [ApiPaths::V0, ApiPaths::V1, ApiPaths::V2] {
        let mut supported = HashSet::default();

        macro_rules! insert {
            ($ty:ty) => {
                if <$ty>::API_PATHS.contains(version) {
                    supported.insert(<$ty>::NAME);
                    if let Some(alias) = <$ty>::NAME_ALIAS {
                        supported.insert(alias);
                    }
                }
            };
        }

        for_each_rpc_method!(insert);

        map.insert(version, supported);
    }

    map
});

/// JSON-RPC middleware layer for segregating RPC methods by the versions they support.
#[derive(Clone, Default)]
pub(super) struct SegregationLayer;

impl<S> Layer<S> for SegregationLayer {
    type Service = SegregationService<S>;

    fn layer(&self, service: S) -> Self::Service {
        SegregationService { service }
    }
}

#[derive(Clone)]
pub(super) struct SegregationService<S> {
    service: S,
}

impl<S> SegregationService<S> {
    fn check<'a>(&self, path: Option<&ApiPaths>, method_name: &str) -> Result<(), ErrorObject<'a>> {
        let supported = path
            .and_then(|p| VERSION_METHODS_MAPPINGS.get(p))
            .map(|set| set.contains(method_name))
            .unwrap_or(false);
        if supported {
            Ok(())
        } else {
            Err(ErrorObject::borrowed(
                http::StatusCode::NOT_FOUND.as_u16() as _,
                "Not Found",
                None,
            ))
        }
    }
}

impl<S> RpcServiceT for SegregationService<S>
where
    S: RpcServiceT<
            MethodResponse = MethodResponse,
            NotificationResponse = MethodResponse,
            BatchResponse = MethodResponse,
        > + Send
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
        match self.check(req.extensions().get::<ApiPaths>(), req.method_name()) {
            Ok(()) => Either::Left(self.service.call(req)),
            Err(e) => Either::Right(async move { MethodResponse::error(req.id(), e) }),
        }
    }

    fn batch<'a>(&self, batch: Batch<'a>) -> impl Future<Output = Self::BatchResponse> + Send + 'a {
        let entries = batch
            .into_iter()
            .filter_map(|entry| match entry {
                Ok(BatchEntry::Call(req)) => Some(
                    match self.check(req.extensions().get::<ApiPaths>(), req.method_name()) {
                        Ok(()) => Ok(BatchEntry::Call(req)),
                        Err(e) => Err(BatchEntryErr::new(req.id(), e)),
                    },
                ),
                Ok(BatchEntry::Notification(n)) => {
                    match self.check(n.extensions().get::<ApiPaths>(), n.method_name()) {
                        Ok(_) => Some(Ok(BatchEntry::Notification(n))),
                        Err(_) => None,
                    }
                }
                Err(err) => Some(Err(err)),
            })
            .collect_vec();
        self.service.batch(Batch::from(entries))
    }

    fn notification<'a>(
        &self,
        n: Notification<'a>,
    ) -> impl Future<Output = Self::NotificationResponse> + Send + 'a {
        match self.check(n.extensions().get::<ApiPaths>(), n.method_name()) {
            Ok(()) => Either::Left(self.service.notification(n)),
            Err(e) => Either::Right(async move { MethodResponse::error(Id::Null, e) }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_methods_mappings() {
        assert!(!VERSION_METHODS_MAPPINGS.is_empty());
    }
}
