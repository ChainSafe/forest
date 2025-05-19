// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::sync::Arc;

use futures::future::Either;
use itertools::Itertools as _;
use jsonrpsee::MethodResponse;
use jsonrpsee::core::middleware::{Batch, BatchEntry, BatchEntryErr, Notification};
use jsonrpsee::server::middleware::rpc::RpcServiceT;
use jsonrpsee::types::{ErrorObject, Id};
use tower::Layer;

use super::FilterList;

/// JSON-RPC middleware layer for filtering RPC methods based on their name.
#[derive(Clone, Default)]
pub(super) struct FilterLayer {
    filter_list: Arc<FilterList>,
}

impl FilterLayer {
    pub fn new(filter_list: FilterList) -> Self {
        Self {
            filter_list: Arc::new(filter_list),
        }
    }
}

impl<S> Layer<S> for FilterLayer {
    type Service = Filtering<S>;

    fn layer(&self, service: S) -> Self::Service {
        Filtering {
            service,
            filter_list: self.filter_list.clone(),
        }
    }
}

#[derive(Clone)]
pub(super) struct Filtering<S> {
    service: S,
    filter_list: Arc<FilterList>,
}

impl<S> Filtering<S> {
    fn authorize<'a>(&self, method_name: &str) -> Result<(), ErrorObject<'a>> {
        if self.filter_list.authorize(method_name) {
            Ok(())
        } else {
            Err(ErrorObject::borrowed(
                http::StatusCode::FORBIDDEN.as_u16() as _,
                "Forbidden",
                None,
            ))
        }
    }
}

impl<S> RpcServiceT for Filtering<S>
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
        match self.authorize(req.method_name()) {
            Ok(()) => Either::Left(self.service.call(req)),
            Err(e) => Either::Right(async move { MethodResponse::error(req.id(), e) }),
        }
    }

    fn notification<'a>(
        &self,
        n: Notification<'a>,
    ) -> impl Future<Output = Self::NotificationResponse> + Send + 'a {
        match self.authorize(n.method_name()) {
            Ok(()) => Either::Left(self.service.notification(n)),
            Err(e) => Either::Right(async move { MethodResponse::error(Id::Null, e) }),
        }
    }

    fn batch<'a>(&self, batch: Batch<'a>) -> impl Future<Output = Self::BatchResponse> + Send + 'a {
        let entries = batch
            .into_iter()
            .filter_map(|entry| match entry {
                Ok(BatchEntry::Call(req)) => Some(match self.authorize(req.method_name()) {
                    Ok(()) => Ok(BatchEntry::Call(req)),
                    Err(e) => Err(BatchEntryErr::new(req.id(), e)),
                }),
                Ok(BatchEntry::Notification(n)) => match self.authorize(n.method_name()) {
                    Ok(_) => Some(Ok(BatchEntry::Notification(n))),
                    Err(_) => None,
                },
                Err(err) => Some(Err(err)),
            })
            .collect_vec();
        self.service.batch(Batch::from(entries))
    }
}
