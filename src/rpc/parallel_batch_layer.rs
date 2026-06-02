// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{borrow::Cow, sync::Arc};

use ahash::HashMap;
use jsonrpsee::{
    MethodResponse,
    core::middleware::{Batch, BatchEntry, Notification},
    server::{BatchResponseBuilder, middleware::rpc::RpcServiceT},
    types::{ErrorCode, ErrorObject, Id, Request},
};
use tokio::task::JoinSet;
use tower::Layer;

/// Parallelize batch RPC requests across the `tokio` worker pool.
///
/// jsonrpsee processes batches sequentially by default. The
/// [JSON-RPC spec](https://www.jsonrpc.org/specification#batch) does not
/// require sequential processing or response ordering, but order is
/// preserved here to avoid surprising clients.
#[derive(Clone, derive_more::Constructor)]
pub(super) struct ParallelBatchLayer {
    max_response_body_size: usize,
}

impl<S> Layer<S> for ParallelBatchLayer {
    type Service = ParallelBatchService<S>;

    fn layer(&self, service: S) -> Self::Service {
        ParallelBatchService {
            service: Arc::new(service),
            max_response_body_size: self.max_response_body_size,
        }
    }
}

#[derive(Clone)]
pub(super) struct ParallelBatchService<S> {
    service: Arc<S>,
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

    fn call<'a>(&self, req: Request<'a>) -> impl Future<Output = Self::MethodResponse> + Send + 'a {
        self.service.call(req)
    }

    fn batch<'a>(&self, batch: Batch<'a>) -> impl Future<Output = Self::BatchResponse> + Send + 'a {
        let max = self.max_response_body_size;
        let mut got_notification = false;
        // JoinSet aborts in-flight tasks on drop.
        let mut join_set: JoinSet<(usize, Option<MethodResponse>)> = JoinSet::new();
        // Lets a panicked call task be turned into a per-entry error with the
        // original request id.
        let mut call_meta: HashMap<tokio::task::Id, (usize, Id<'static>)> = HashMap::default();
        let mut results: Vec<(usize, Option<MethodResponse>)> = Vec::new();

        for (idx, entry) in batch.into_iter().enumerate() {
            let service = Arc::clone(&self.service);
            match entry {
                Ok(BatchEntry::Call(req)) => {
                    let req_id = req.id().into_owned();
                    let req = into_owned_request(req);
                    let handle =
                        join_set.spawn(async move { (idx, Some(service.call(req).await)) });
                    call_meta.insert(handle.id(), (idx, req_id));
                }
                Ok(BatchEntry::Notification(n)) => {
                    got_notification = true;
                    let n = into_owned_notification(n);
                    join_set.spawn(async move {
                        service.notification(n).await;
                        (idx, None)
                    });
                }
                Err(err) => {
                    let (err, id) = err.into_parts();
                    results.push((
                        idx,
                        Some(MethodResponse::error(id.into_owned(), err.into_owned())),
                    ));
                }
            }
        }

        async move {
            results.reserve(join_set.len());
            while let Some(joined) = join_set.join_next_with_id().await {
                match joined {
                    Ok((_, r)) => results.push(r),
                    Err(e) if e.is_panic() => {
                        if let Some((idx, req_id)) = call_meta.remove(&e.id()) {
                            tracing::error!(idx, "RPC call panicked in batch entry");
                            let err = ErrorObject::owned::<()>(
                                ErrorCode::InternalError.code(),
                                "RPC handler panicked",
                                None,
                            );
                            results.push((idx, Some(MethodResponse::error(req_id, err))));
                        } else {
                            tracing::error!("RPC notification panicked in batch entry");
                        }
                    }
                    Err(_) => unreachable!("JoinSet only cancels tasks on drop"),
                }
            }
            results.sort_by_key(|(idx, _)| *idx);

            let mut batch_rp = BatchResponseBuilder::new_with_limit(max);
            for (_, rp) in results {
                if let Some(rp) = rp
                    && let Err(err) = batch_rp.append(rp)
                {
                    return err;
                }
            }

            // Empty builder + at least one notification is the spec's
            // "no response" case for a notification-only batch.
            if batch_rp.is_empty() && got_notification {
                MethodResponse::notification()
            } else {
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

fn into_owned_request(req: Request<'_>) -> Request<'static> {
    Request {
        jsonrpc: req.jsonrpc,
        id: req.id.into_owned(),
        method: Cow::Owned(req.method.into_owned()),
        params: req.params.map(|p| Cow::Owned(p.into_owned())),
        extensions: req.extensions,
    }
}

fn into_owned_notification(n: Notification<'_>) -> Notification<'static> {
    Notification {
        jsonrpc: n.jsonrpc,
        method: Cow::Owned(n.method.into_owned()),
        params: n.params.map(|p| Cow::Owned(p.into_owned())),
        extensions: n.extensions,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jsonrpsee::core::middleware::BatchEntryErr;
    use jsonrpsee::server::ResponsePayload;
    use jsonrpsee::types::{Extensions, TwoPointZero};
    use std::time::Duration;

    const MAX_RESP: usize = 1024 * 1024;

    /// Method conventions used by tests:
    ///   "ok"        – success response carrying the method name.
    ///   "slow:<ms>" – sleep, then succeed.
    ///   "panic"     – panic inside the call task.
    #[derive(Clone, Default)]
    struct TestService;

    impl RpcServiceT for TestService {
        type MethodResponse = MethodResponse;
        type NotificationResponse = MethodResponse;
        type BatchResponse = MethodResponse;

        fn call<'a>(
            &self,
            req: Request<'a>,
        ) -> impl Future<Output = Self::MethodResponse> + Send + 'a {
            let id = req.id().into_owned();
            let method = req.method_name().to_string();
            async move {
                if method == "panic" {
                    panic!("test panic");
                }
                if let Some(rest) = method.strip_prefix("slow:") {
                    let ms: u64 = rest.parse().unwrap();
                    tokio::time::sleep(Duration::from_millis(ms)).await;
                }
                MethodResponse::response(id, ResponsePayload::success(method), MAX_RESP)
            }
        }

        // `async fn` form drops the explicit `'a` capture the trait wants,
        // and the `manual_async_fn` lint fires on trivial `async {}` bodies.
        #[expect(clippy::manual_async_fn, reason = "trait demands explicit 'a")]
        fn batch<'a>(
            &self,
            _b: Batch<'a>,
        ) -> impl Future<Output = Self::BatchResponse> + Send + 'a {
            async { unreachable!("ParallelBatchLayer overrides this") }
        }

        #[expect(clippy::manual_async_fn, reason = "trait demands explicit 'a")]
        fn notification<'a>(
            &self,
            _n: Notification<'a>,
        ) -> impl Future<Output = Self::NotificationResponse> + Send + 'a {
            async { MethodResponse::notification() }
        }
    }

    fn layer() -> ParallelBatchService<TestService> {
        ParallelBatchService {
            service: Arc::new(TestService),
            max_response_body_size: MAX_RESP,
        }
    }

    fn call(id: u64, method: &str) -> Request<'static> {
        Request::owned(method.to_string(), None, Id::Number(id))
    }

    fn notification(method: &str) -> Notification<'static> {
        Notification {
            jsonrpc: TwoPointZero,
            method: Cow::Owned(method.to_string()),
            params: None,
            extensions: Extensions::new(),
        }
    }

    fn as_array(rp: &MethodResponse) -> Vec<serde_json::Value> {
        serde_json::from_str::<Vec<serde_json::Value>>(rp.as_json().get()).unwrap()
    }

    #[tokio::test]
    async fn preserves_order_under_heterogeneous_latency() {
        let svc = layer();
        let batch = Batch::from(vec![
            Ok(BatchEntry::Call(call(1, "slow:50"))),
            Ok(BatchEntry::Call(call(2, "ok"))),
            Ok(BatchEntry::Call(call(3, "slow:25"))),
        ]);
        let arr = as_array(&svc.batch(batch).await);
        assert_eq!(arr.len(), 3);
        assert_eq!(arr[0]["id"], 1);
        assert_eq!(arr[1]["id"], 2);
        assert_eq!(arr[2]["id"], 3);
    }

    #[tokio::test]
    async fn panicked_call_yields_per_entry_error() {
        let svc = layer();
        let batch = Batch::from(vec![
            Ok(BatchEntry::Call(call(1, "ok"))),
            Ok(BatchEntry::Call(call(2, "panic"))),
            Ok(BatchEntry::Call(call(3, "ok"))),
        ]);
        let arr = as_array(&svc.batch(batch).await);
        assert_eq!(arr.len(), 3);
        assert_eq!(arr[0]["id"], 1);
        assert!(arr[0]["result"].is_string(), "first entry should succeed");
        assert_eq!(arr[1]["id"], 2);
        assert!(
            arr[1]["error"].is_object(),
            "panicked entry must carry its own error"
        );
        assert_eq!(arr[2]["id"], 3);
        assert!(arr[2]["result"].is_string(), "third entry should succeed");
    }

    #[tokio::test]
    async fn notification_only_batch_returns_notification() {
        let svc = layer();
        let batch = Batch::from(vec![Ok(BatchEntry::Notification(notification("ok")))]);
        let resp = svc.batch(batch).await;
        assert!(resp.is_notification());
    }

    #[tokio::test]
    async fn entry_err_preserves_index() {
        let svc = layer();
        let batch = Batch::from(vec![
            Ok(BatchEntry::Call(call(1, "ok"))),
            Err(BatchEntryErr::new(
                Id::Number(2),
                ErrorObject::from(ErrorCode::InvalidRequest),
            )),
            Ok(BatchEntry::Call(call(3, "ok"))),
        ]);
        let arr = as_array(&svc.batch(batch).await);
        assert_eq!(arr.len(), 3);
        assert_eq!(arr[0]["id"], 1);
        assert_eq!(arr[1]["id"], 2);
        assert!(arr[1]["error"].is_object());
        assert_eq!(arr[2]["id"], 3);
    }
}
