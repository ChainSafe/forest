// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use futures::{FutureExt, StreamExt, stream::FuturesOrdered};
use jsonrpsee::{
    MethodResponse,
    core::middleware::{Batch, BatchEntry, Notification},
    server::{BatchResponseBuilder, middleware::rpc::RpcServiceT},
};
use tower::Layer;

/// Parallelize batch RPC requests that are processed in sequence by default
/// See <https://github.com/paritytech/jsonrpsee/blob/v0.26.0/server/src/middleware/rpc.rs#L157>
///
/// Note that such parallelization is allowed as per the [`JSON-RPC` specification](https://www.jsonrpc.org/specification#:~:text=6%20Batch)
#[derive(Clone, derive_more::Constructor)]
pub(super) struct ParallelBatchLayer {
    max_response_body_size: usize,
}

impl<S> Layer<S> for ParallelBatchLayer {
    type Service = ParallelBatchService<S>;

    fn layer(&self, service: S) -> Self::Service {
        ParallelBatchService {
            service,
            max_response_body_size: self.max_response_body_size,
        }
    }
}

#[derive(Clone)]
pub(super) struct ParallelBatchService<S> {
    service: S,
    max_response_body_size: usize,
}

impl<S> RpcServiceT for ParallelBatchService<S>
where
    S: RpcServiceT<
            MethodResponse = MethodResponse,
            NotificationResponse = MethodResponse,
            BatchResponse = MethodResponse,
        > + Send
        + Sync
        + 'static,
{
    type MethodResponse = S::MethodResponse;
    type NotificationResponse = S::NotificationResponse;
    type BatchResponse = S::BatchResponse;

    fn call<'a>(
        &self,
        req: jsonrpsee::types::Request<'a>,
    ) -> impl Future<Output = Self::MethodResponse> + Send + 'a {
        self.service.call(req)
    }

    // Parallelized version of https://github.com/paritytech/jsonrpsee/blob/v0.26.0/server/src/middleware/rpc.rs#L151
    fn batch<'a>(&self, batch: Batch<'a>) -> impl Future<Output = Self::BatchResponse> + Send + 'a {
        // Process batch in parallel instead of delegating to the inner service, which processes them sequentially.
        let mut batch_rp = BatchResponseBuilder::new_with_limit(self.max_response_body_size);
        let mut got_notification = false;
        // Although it's not neccesary to perserve the order in response, we do it to avoid potential bugs on client side
        // See <https://www.jsonrpc.org/specification#:~:text=6%20Batch>
        let mut tasks = FuturesOrdered::new();
        for batch_entry in batch.into_iter() {
            match batch_entry {
                Ok(BatchEntry::Call(req)) => {
                    tasks.push_back(self.service.call(req).map(Some).boxed());
                }
                Ok(BatchEntry::Notification(n)) => {
                    got_notification = true;
                    tasks.push_back(self.service.notification(n).map(|_| None).boxed());
                }
                Err(err) => {
                    let (err, id) = err.into_parts();
                    let rp = MethodResponse::error(id, err);
                    tasks.push_back(async move { Some(rp) }.boxed());
                }
            }
        }

        async move {
            while let Some(r) = tasks.next().await {
                if let Some(rp) = r
                    && let Err(err) = batch_rp.append(rp)
                {
                    return err;
                }
            }

            // If the batch is empty and we got a notification, we return an empty response.
            if batch_rp.is_empty() && got_notification {
                MethodResponse::notification()
            }
            // An empty batch is regarded as an invalid request here.
            else {
                MethodResponse::from_batch(batch_rp.finish())
            }
        }
    }

    fn notification<'a>(
        &self,
        n: Notification<'a>,
    ) -> impl Future<Output = Self::NotificationResponse> + Send + 'a {
        self.service.notification(n)
    }
}
