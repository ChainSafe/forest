// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::sync::Arc;

use futures::future::Either;
use itertools::Itertools as _;
use jsonrpsee::MethodResponse;
use jsonrpsee::core::middleware::{Batch, BatchEntry, BatchEntryErr, Notification};
use jsonrpsee::server::middleware::rpc::RpcServiceT;
use jsonrpsee::types::{ErrorObject, Id};
use tower::Layer;

use super::{CANCEL_METHOD_NAME, FilterList};

/// JSON-RPC middleware layer for filtering RPC methods based on their name.
#[derive(Clone, Default)]
pub(super) struct FilterLayer {
    filter_list: Arc<FilterList>,
}

impl FilterLayer {
    pub fn new(filter_list: Arc<FilterList>) -> Self {
        Self { filter_list }
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
        // `xrpc.cancel` is channel protocol machinery, not a callable API method;
        // Lotus answers it from the frame switch, where no filtering applies:
        // <https://github.com/filecoin-project/go-jsonrpc/blob/v0.10.2/websocket.go#L592-L603>
        // It is exempt from the allow list only: an operator who rejects it
        // outright is refusing something, not merely failing to list it.
        let authorized = if method_name == CANCEL_METHOD_NAME {
            !self.filter_list.is_rejected(method_name)
        } else {
            self.filter_list.authorize(method_name)
        };
        if authorized {
            Ok(())
        } else {
            Err(ErrorObject::borrowed(
                i32::from(http::StatusCode::FORBIDDEN.as_u16()),
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

#[cfg(test)]
mod tests {
    use super::*;

    /// An allow-list that does not name `xrpc.cancel` must still let it
    /// through, or a filtered node's channels can never be cancelled.
    #[test]
    fn cancel_is_exempt_from_the_allow_list() {
        let filtering = Filtering {
            service: (),
            filter_list: Arc::new(FilterList::default().allow("Filecoin.ChainNotify".into())),
        };

        assert!(filtering.authorize(CANCEL_METHOD_NAME).is_ok());
        assert!(filtering.authorize("Filecoin.ChainNotify").is_ok());
        assert!(filtering.authorize("Filecoin.ChainHead").is_err());
    }

    /// The exemption covers the allow list only — an explicit reject still wins,
    /// so an operator's deny is never silently discarded.
    #[test]
    fn cancel_still_obeys_the_reject_list() {
        let filtering = Filtering {
            service: (),
            filter_list: Arc::new(FilterList::default().reject(CANCEL_METHOD_NAME.into())),
        };

        assert!(filtering.authorize(CANCEL_METHOD_NAME).is_err());
    }
}
