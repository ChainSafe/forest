// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
//! Subscription related types and traits for server implementations.
//!
//! Most of the code in this module comes from the `jsonrpsee` crate.
//! See <https://github.com/paritytech/jsonrpsee/blob/v0.21.0/core/src/server/subscription.rs>.
//! We slightly customized it from the original design to support Filecoin `pubsub` specification.
//! The principal changed types are the `PendingSubscriptionSink` and `SubscriptionSink`, adding an `u64` channel identifier member.
//!
//! The remaining types and methods must be duplicated because they are private.
//!
//! The sequence diagram of a channel lifetime is as follows:
//! ```text
//!  ┌─────────────┐                                                       ┌─────────────┐
//!  │  WS Client  │                                                       │    Node     │
//!  └─────────────┘                                                       └─────────────┘
//!         │                                                                     │
//!         │  ┌────────────────────────────────┐                                 │
//!         │──┤ Subscription message           ├───────────────────────────────▶ │
//!         │  │                                │                                 │
//!         │  │{ jsonrpc:'2.0',                │                                 │
//!         │  │  id:<id>,                      │                                 │
//!         │  │  method:'Filecoin.ChainNotify',│                                 │
//!         │  │  params:[] }                   │                                 │
//!         │  └────────────────────────────────┘                                 │
//!         │                                 ┌────────────────────────────────┐  │
//!         │ ◀───────────────────────────────┤ Opened channel message         ├──│
//!         │                                 │                                │  │
//!         │                                 │{ jsonrpc:'2.0',                │  │
//!         │                                 │  result:<channId>,             │  │
//!         │                                 │  id:<id> }                     │  │
//!         │                                 └────────────────────────────────┘  │
//!         │                                                                     │
//!         │                                                                     │
//!         │                                 ┌────────────────────────────────┐  │
//!         │ ◀───────────────────────────────┤ Notification message           ├──│
//!         │                                 │                                │  │
//!         │                                 │{ jsonrpc:'2.0',                │  │
//!         │                                 │  method:'xrpc.ch.val',         │  │
//!         │                                 │  params:[<channId>,<payload>] }│  │
//!         │                                 └────────────────────────────────┘  │
//!         │                                                                     │
//!         │                                                                     │
//!         │                                                                     │
//!         │                      After a few notifications                      │
//!         │  ┌────────────────────────────────┐                                 │
//!         │──┤ Cancel subscription            ├───────────────────────────────▶ │
//!         │  │                                │                                 │
//!         │  │{ jsonrpc:'2.0',                │                                 │
//!         │  │  method:'xrpc.cancel',         │                                 │
//!         │  │  params:[<id>],                │                                 │
//!         │  │  id:null }                     │                                 │
//!         │  └────────────────────────────────┘                                 │
//!         │                                 ┌────────────────────────────────┐  │
//!         │ ◀───────────────────────────────┤ Closed channel message         ├──│
//!         │                                 │                                │  │
//!         │                                 │{ jsonrpc:'2.0',                │  │
//!         │                                 │  method:'xrpc.ch.close',       │  │
//!         │                                 │  params:[<channId>] }          │  │
//!         │                                 └────────────────────────────────┘  │
//! ```

use ahash::HashMap;
use jsonrpsee::{
    ConnectionId, MethodResponse, MethodSink,
    server::{
        IntoSubscriptionCloseResponse, MethodCallback, Methods, RegisterMethodError,
        ResponsePayload,
    },
    types::{ErrorObjectOwned, Id, Params, error::ErrorCode},
};
use parking_lot::Mutex;
use serde_json::value::{RawValue, to_raw_value};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::{mpsc, oneshot};

use super::error::ServerError;

pub const NOTIF_METHOD_NAME: &str = "xrpc.ch.val";
pub const CANCEL_METHOD_NAME: &str = "xrpc.cancel";

pub type ChannelId = u64;

/// Type-alias for subscribers.
pub type Subscribers =
    Arc<Mutex<HashMap<(ConnectionId, Id<'static>), (MethodSink, mpsc::Receiver<()>, ChannelId)>>>;

/// Represents a single subscription that is waiting to be accepted or rejected.
///
/// If this is dropped without calling `PendingSubscription::reject` or `PendingSubscriptionSink::accept`
/// a default error is sent out as response to the subscription call.
///
/// Thus, if you want a customized error message then `PendingSubscription::reject` must be called.
#[derive(Debug)]
#[must_use = "PendingSubscriptionSink does nothing unless `accept` or `reject` is called"]
pub struct PendingSubscriptionSink {
    /// Sink.
    pub(crate) inner: MethodSink,
    /// `MethodCallback`.
    pub(crate) method: &'static str,
    /// Shared Mutex of subscriptions for this method.
    pub(crate) subscribers: Subscribers,
    /// ID of the `subscription call` (i.e. not the same as subscription id) which is used
    /// to reply to subscription method call and must only be used once.
    pub(crate) id: Id<'static>,
    /// Sender to answer the subscribe call.
    pub(crate) subscribe: oneshot::Sender<MethodResponse>,
    /// Channel identifier.
    pub(crate) channel_id: ChannelId,
    /// Connection identifier.
    pub(crate) connection_id: ConnectionId,
}

impl PendingSubscriptionSink {
    /// Attempt to accept the subscription and respond the subscription method call.
    ///
    /// # Panics
    ///
    /// Panics if the subscription response exceeded the `max_response_size`.
    pub async fn accept(self) -> Result<SubscriptionSink, String> {
        let channel_id = self.channel_id();
        let id = self.id.clone();
        let response = MethodResponse::subscription_response(
            self.id,
            ResponsePayload::success_borrowed(&channel_id),
            self.inner.max_response_size() as usize,
        );
        let success = response.is_success();

        // Ideally the message should be sent only once.
        //
        // The same message is sent twice here because one is sent directly to the transport layer and
        // the other one is sent internally to accept the subscription.
        self.inner
            .send(response.to_json())
            .await
            .map_err(|e| e.to_string())?;
        self.subscribe
            .send(response)
            .map_err(|e| format!("accept error: {}", e.as_json()))?;

        if success {
            let (tx, rx) = mpsc::channel(1);
            self.subscribers.lock().insert(
                (self.connection_id, id),
                (self.inner.clone(), rx, self.channel_id),
            );
            tracing::debug!(
                "Accepting subscription (conn_id={}, chann_id={})",
                self.connection_id.0,
                self.channel_id
            );
            Ok(SubscriptionSink {
                inner: self.inner,
                method: self.method,
                unsubscribe: IsUnsubscribed(tx),
                channel_id: self.channel_id,
            })
        } else {
            panic!(
                "The subscription response was too big; adjust the `max_response_size` or change Subscription ID generation"
            );
        }
    }

    /// Returns the channel identifier
    pub fn channel_id(&self) -> ChannelId {
        self.channel_id
    }
}

/// Represents a subscription until it is unsubscribed.
#[derive(Debug, Clone)]
pub struct IsUnsubscribed(mpsc::Sender<()>);

impl IsUnsubscribed {
    /// Wrapper over [`tokio::sync::mpsc::Sender::closed`]
    pub async fn unsubscribed(&self) {
        self.0.closed().await;
    }
}

/// Represents a single subscription that hasn't been processed yet.
#[derive(Debug, Clone)]
pub struct SubscriptionSink {
    /// Sink.
    inner: MethodSink,
    /// `MethodCallback`.
    method: &'static str,
    /// A future that fires once the unsubscribe method has been called.
    unsubscribe: IsUnsubscribed,
    /// Channel identifier.
    channel_id: ChannelId,
}

impl SubscriptionSink {
    /// Get the method name.
    pub fn method_name(&self) -> &str {
        self.method
    }

    /// Get the channel ID.
    pub fn channel_id(&self) -> ChannelId {
        self.channel_id
    }

    /// Send out a response on the subscription and wait until there is capacity.
    ///
    ///
    /// Returns
    /// - `Ok(())` if the message could be sent.
    /// - `Err(unsent_msg)` if the connection or subscription was closed.
    ///
    /// # Cancel safety
    ///
    /// This method is cancel-safe and dropping a future loses its spot in the waiting queue.
    pub async fn send(&self, msg: Box<serde_json::value::RawValue>) -> Result<(), String> {
        // Only possible to trigger when the connection is dropped.
        if self.is_closed() {
            return Err(format!("disconnect error: {msg}"));
        }

        self.inner.send(msg).await.map_err(|e| e.to_string())
    }

    /// Returns whether the subscription is closed.
    pub fn is_closed(&self) -> bool {
        self.inner.is_closed()
    }

    /// Completes when the subscription has been closed.
    pub async fn closed(&self) {
        // Both are cancel-safe thus ok to use select here.
        tokio::select! {
            _ = self.inner.closed() => (),
            _ = self.unsubscribe.unsubscribed() => (),
        }
    }
}

fn create_notif_message(
    sink: &SubscriptionSink,
    result: &impl serde::Serialize,
) -> anyhow::Result<Box<RawValue>> {
    let method = sink.method_name();
    let channel_id = sink.channel_id();
    let result = serde_json::to_value(result)?;
    let msg = serde_json::json!({
        "jsonrpc": "2.0",
        "method": method,
        "params": [channel_id, result]
    });

    tracing::debug!("Sending notification: {}", msg);

    Ok(to_raw_value(&msg)?)
}

fn close_payload(channel_id: ChannelId) -> serde_json::Value {
    serde_json::json!({
        "jsonrpc":"2.0",
        "method":"xrpc.ch.close",
        "params":[channel_id]
    })
}

fn close_channel_response(channel_id: ChannelId) -> MethodResponse {
    MethodResponse::response(
        Id::Null,
        ResponsePayload::success(close_payload(channel_id)),
        1024,
    )
}

/// Sends the bare `xrpc.ch.close` notification for this channel, ignoring
/// send failures (the connection may already be gone).
async fn send_close(sink: &SubscriptionSink) {
    if let Ok(payload) = to_raw_value(&close_payload(sink.channel_id())) {
        let _ = sink.send(payload).await;
    }
}

#[derive(Debug, Clone)]
pub struct RpcModule {
    id_provider: Arc<AtomicU64>,
    channels: Subscribers,
    methods: Methods,
}

impl From<RpcModule> for Methods {
    fn from(module: RpcModule) -> Methods {
        module.methods
    }
}

impl Default for RpcModule {
    fn default() -> Self {
        let mut methods = Methods::default();

        let channels = Subscribers::default();
        methods
            .verify_and_insert(
                CANCEL_METHOD_NAME,
                MethodCallback::Unsubscription(Arc::new({
                    let channels = channels.clone();
                    move |id,
                          params: Params,
                          connection_id: ConnectionId,
                          _max_response,
                          _extensions| {
                        let cb = || {
                            let [id]: [Id<'_>; 1] = params.parse()?;
                            let sub_id = id.into_owned();

                            tracing::debug!("Got cancel request (id={sub_id})");

                            let opt = channels.lock().remove(&(connection_id, sub_id));
                            match opt {
                                Some((_, _, channel_id)) => {
                                    Ok::<ChannelId, ServerError>(channel_id)
                                }
                                None => Err::<ChannelId, ServerError>(ServerError::from(
                                    anyhow::anyhow!("channel not found"),
                                )),
                            }
                        };
                        let result = cb();
                        match result {
                            Ok(channel_id) => {
                                let resp = close_channel_response(channel_id);
                                tracing::debug!("Sending close message: {}", resp.as_json());
                                resp
                            }
                            Err(e) => {
                                let error: ErrorObjectOwned = e.into();
                                MethodResponse::error(id, error)
                            }
                        }
                    }
                })),
            )
            .expect("Inserting a method into an empty methods map is infallible.");

        Self {
            id_provider: Arc::new(AtomicU64::new(0)),
            channels,
            methods,
        }
    }
}

impl RpcModule {
    pub fn register_channel<R, F>(
        &mut self,
        subscribe_method_name: &'static str,
        callback: F,
    ) -> Result<&mut MethodCallback, RegisterMethodError>
    where
        F: (Fn(Params) -> tokio::sync::broadcast::Receiver<R>) + Send + Sync + 'static,
        R: serde::Serialize + Clone + Send + 'static,
    {
        self.register_channel_raw(subscribe_method_name, {
            move |params, pending| {
                let mut receiver = callback(params);
                tokio::spawn(async move {
                    let sink = if let Ok(sink) = pending.accept().await {
                        sink
                    } else {
                        tracing::error!("Failed to accept subscription");
                        return;
                    };
                    tracing::debug!("Channel created: chann_id={}", sink.channel_id);

                    loop {
                        tokio::select! {
                            action = receiver.recv() => {
                                match action {
                                    Ok(msg) => {
                                        match create_notif_message(&sink, &msg) {
                                            Ok(msg) => {
                                                if let Err(e) = sink.send(msg).await {
                                                    tracing::error!("Failed to send message: {:?}", e);
                                                    break;
                                                }
                                            }
                                            Err(e) => {
                                                tracing::error!("Failed to serialize channel message: {:?}", e);
                                                break;
                                            }
                                        }
                                    }
                                    Err(RecvError::Closed) => {
                                        send_close(&sink).await;
                                        break;
                                    }
                                    Err(RecvError::Lagged(n)) => {
                                        // Events were lost: close the channel (like Lotus)
                                        // so the client knows to resubscribe and resync,
                                        // instead of silently continuing with a gap.
                                        tracing::warn!(
                                            "closing channel {}: subscriber lagged by {n} messages",
                                            sink.channel_id()
                                        );
                                        send_close(&sink).await;
                                        break;
                                    }
                                }
                            },
                            _ = sink.closed() => {
                                break;
                            }
                        }
                    }

                    tracing::debug!("Send notification task ended (chann_id={})", sink.channel_id);
                });
            }
        })
    }

    fn register_channel_raw<R, F>(
        &mut self,
        subscribe_method_name: &'static str,
        callback: F,
    ) -> Result<&mut MethodCallback, RegisterMethodError>
    where
        F: (Fn(Params, PendingSubscriptionSink) -> R) + Send + Sync + 'static,
        R: IntoSubscriptionCloseResponse,
    {
        self.methods.verify_method_name(subscribe_method_name)?;
        let subscribers = self.channels.clone();

        // Subscribe
        self.methods.verify_and_insert(
            subscribe_method_name,
            MethodCallback::Subscription(Arc::new({
                let id_provider = self.id_provider.clone();
                move |id, params, method_sink, conn, _extensions| {
                    let channel_id = id_provider.fetch_add(1, Ordering::Relaxed);

                    // response to the subscription call.
                    let (tx, rx) = oneshot::channel();

                    let sink = PendingSubscriptionSink {
                        inner: method_sink,
                        method: NOTIF_METHOD_NAME,
                        subscribers: subscribers.clone(),
                        id: id.clone().into_owned(),
                        subscribe: tx,
                        channel_id,
                        connection_id: conn.conn_id,
                    };

                    callback(params, sink);

                    let id = id.into_owned();

                    Box::pin(async move {
                        match rx.await {
                            Ok(rp) => rp,
                            Err(_) => MethodResponse::error(id, ErrorCode::InternalError),
                        }
                    })
                }
            })),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};
    use std::time::Duration;
    use tokio::sync::broadcast;

    const TEST_METHOD: &str = "test.channel";
    const RECV_TIMEOUT: Duration = Duration::from_secs(5);
    /// Capacity of the per-test event source; the lag test overflows it to
    /// force a `Lagged` observation.
    const SOURCE_CAPACITY: usize = 4;
    /// Buffer size of the per-call frame stream returned by `raw_json_request`.
    const STREAM_BUF_SIZE: usize = 256;

    /// A [`Methods`] with one channel method: every subscriber gets a fresh
    /// receiver from the same `events` broadcast source.
    ///
    /// The callback keeps only a receiver prototype — not a sender clone — so
    /// the test's `events` sender stays the single sender and dropping it
    /// closes the source (exercised by the close tests).
    fn test_methods(events: &broadcast::Sender<String>) -> Methods {
        let mut module = RpcModule::default();
        let prototype = events.subscribe();
        module
            .register_channel(TEST_METHOD, move |_params| prototype.resubscribe())
            .unwrap();
        module.into()
    }

    /// Subscribe with the given request id; returns the allocated channel id
    /// and the stream of frames sent to this "connection".
    ///
    /// Every `raw_json_request` call gets its own frame stream, but they all
    /// share `ConnectionId(0)`. The duplicate subscribe response that
    /// `accept()` writes to the transport sink is swallowed by
    /// `raw_json_request` itself, so the stream carries notification frames
    /// only.
    async fn subscribe(
        methods: &Methods,
        request_id: u64,
    ) -> (ChannelId, mpsc::Receiver<Box<RawValue>>) {
        let request = format!(
            r#"{{"jsonrpc":"2.0","id":{request_id},"method":"{TEST_METHOD}","params":[]}}"#
        );
        let (response, frames) = methods
            .raw_json_request(&request, STREAM_BUF_SIZE)
            .await
            .unwrap();
        let response: Value = serde_json::from_str(response.get()).unwrap();
        assert_eq!(response.get("id"), Some(&json!(request_id)));
        let channel_id = response
            .get("result")
            .and_then(Value::as_u64)
            .unwrap_or_else(|| panic!("channel id must be a bare u64: {response}"));
        (channel_id, frames)
    }

    /// Request id used for the `xrpc.cancel` calls themselves; non-null so
    /// the error path's id echo is observable.
    const CANCEL_REQUEST_ID: u64 = 999;

    /// Send an `xrpc.cancel` for the given original request id and return the
    /// raw response.
    async fn cancel(methods: &Methods, target_request_id: u64) -> Value {
        let request = format!(
            r#"{{"jsonrpc":"2.0","id":{CANCEL_REQUEST_ID},"method":"{CANCEL_METHOD_NAME}","params":[{target_request_id}]}}"#
        );
        let (response, _) = methods
            .raw_json_request(&request, STREAM_BUF_SIZE)
            .await
            .unwrap();
        serde_json::from_str(response.get()).unwrap()
    }

    async fn next_frame(frames: &mut mpsc::Receiver<Box<RawValue>>) -> Value {
        let frame = tokio::time::timeout(RECV_TIMEOUT, frames.recv())
            .await
            .expect("timed out waiting for a frame")
            .expect("stream closed while waiting for a frame");
        serde_json::from_str(frame.get()).unwrap()
    }

    /// Assert the stream ends without yielding another frame. This only
    /// resolves once the channel's pump task has exited and dropped its sink,
    /// so it doubles as a synchronization point on pump shutdown.
    async fn assert_stream_closed(frames: &mut mpsc::Receiver<Box<RawValue>>) {
        let frame = tokio::time::timeout(RECV_TIMEOUT, frames.recv())
            .await
            .expect("timed out waiting for the stream to close");
        assert!(
            frame.is_none(),
            "expected the stream to close, got frame: {}",
            frame.unwrap().get()
        );
    }

    fn val_frame(channel_id: ChannelId, payload: &str) -> Value {
        json!({"jsonrpc": "2.0", "method": NOTIF_METHOD_NAME, "params": [channel_id, payload]})
    }

    fn close_frame(channel_id: ChannelId) -> Value {
        json!({"jsonrpc": "2.0", "method": "xrpc.ch.close", "params": [channel_id]})
    }

    /// The response shape `xrpc.cancel` currently produces: an `id:null`
    /// response wrapping the close notification (see #4453).
    fn close_response(channel_id: ChannelId) -> Value {
        json!({"jsonrpc": "2.0", "id": null, "result": close_frame(channel_id)})
    }

    #[tokio::test]
    async fn subscribe_returns_u64_channel_id() {
        let (events, _) = broadcast::channel::<String>(SOURCE_CAPACITY);
        let methods = test_methods(&events);

        let (first_channel, _first_frames) = subscribe(&methods, 1).await;
        let (second_channel, _second_frames) = subscribe(&methods, 2).await;

        assert_eq!(second_channel, first_channel + 1);
    }

    #[tokio::test]
    async fn value_framing_positional() {
        let (events, _) = broadcast::channel(SOURCE_CAPACITY);
        let methods = test_methods(&events);
        let (channel_id, mut frames) = subscribe(&methods, 1).await;

        events.send("head-change".into()).unwrap();
        drop(events);

        // Exactly one `xrpc.ch.val` frame with positional params
        // `[channelId, payload]`, then the close from the dropped source —
        // proving the send produced no extra frames.
        assert_eq!(
            next_frame(&mut frames).await,
            val_frame(channel_id, "head-change")
        );
        assert_eq!(next_frame(&mut frames).await, close_frame(channel_id));
    }

    #[tokio::test]
    async fn two_channels_one_conn_independent() {
        let (events, _) = broadcast::channel(SOURCE_CAPACITY);
        let methods = test_methods(&events);
        let (first_channel, mut first_frames) = subscribe(&methods, 1).await;
        let (second_channel, mut second_frames) = subscribe(&methods, 2).await;
        assert_ne!(first_channel, second_channel);

        // both channels deliver the same event
        events.send("both".into()).unwrap();
        assert_eq!(
            next_frame(&mut first_frames).await,
            val_frame(first_channel, "both")
        );
        assert_eq!(
            next_frame(&mut second_frames).await,
            val_frame(second_channel, "both")
        );

        // cancelling #1 closes only #1 (wait for its pump to exit before the
        // next send, so the event cannot race the pump shutdown)
        assert_eq!(cancel(&methods, 1).await, close_response(first_channel));
        assert_stream_closed(&mut first_frames).await;

        // ... while #2 still delivers
        events.send("second-only".into()).unwrap();
        assert_eq!(
            next_frame(&mut second_frames).await,
            val_frame(second_channel, "second-only")
        );
    }

    #[tokio::test]
    async fn hundred_channel_fanout() {
        let (events, _) = broadcast::channel(SOURCE_CAPACITY);
        let methods = test_methods(&events);

        let mut channels = Vec::new();
        for request_id in 1..=100 {
            channels.push(subscribe(&methods, request_id).await);
        }

        events.send("fan-out".into()).unwrap();

        let mut seen = ahash::HashSet::default();
        for (channel_id, frames) in &mut channels {
            assert_eq!(next_frame(frames).await, val_frame(*channel_id, "fan-out"));
            assert!(seen.insert(*channel_id), "channel ids must be unique");
        }
    }

    #[tokio::test]
    async fn cancel_unknown_id_errors() {
        let (events, _) = broadcast::channel(SOURCE_CAPACITY);
        let methods = test_methods(&events);
        let (channel_id, mut frames) = subscribe(&methods, 1).await;

        let response = cancel(&methods, 99).await;
        assert!(
            response.get("error").is_some(),
            "cancelling an unknown id must return an error response: {response}"
        );
        assert!(response.get("result").is_none());
        // the error path echoes the cancel request's own id
        assert_eq!(response.get("id"), Some(&json!(CANCEL_REQUEST_ID)));

        // the live channel is unaffected
        events.send("still-open".into()).unwrap();
        assert_eq!(
            next_frame(&mut frames).await,
            val_frame(channel_id, "still-open")
        );
    }

    /// When the event source closes, the client gets a bare `xrpc.ch.close`
    /// notification.
    #[tokio::test]
    async fn source_closed_sends_bare_close() {
        let (events, _) = broadcast::channel::<String>(SOURCE_CAPACITY);
        let methods = test_methods(&events);
        let (channel_id, mut frames) = subscribe(&methods, 1).await;

        drop(events);

        assert_eq!(next_frame(&mut frames).await, close_frame(channel_id));
    }

    /// Regression test: a subscriber that falls behind the broadcast source
    /// has its channel closed — like Lotus, so the client knows to
    /// resubscribe and re-sync — instead of silently losing the overflowed
    /// events while the channel stays open.
    #[tokio::test]
    async fn lagged_consumer_channel_closes() {
        let (events, lagged_rx) = broadcast::channel(SOURCE_CAPACITY);
        for n in 0..SOURCE_CAPACITY + 2 {
            events.send(format!("event-{n}")).unwrap();
        }
        let lagged_rx = Mutex::new(Some(lagged_rx));
        let mut module = RpcModule::default();
        module
            .register_channel(TEST_METHOD, move |_params| {
                lagged_rx.lock().take().expect("single subscriber")
            })
            .unwrap();
        let methods: Methods = module.into();

        let (channel_id, mut frames) = subscribe(&methods, 1).await;

        // no value frames arrive, the client is told the channel is gone
        assert_eq!(next_frame(&mut frames).await, close_frame(channel_id));

        // the pump exited and dropped its receiver — the source has no
        // subscribers left, and no stray frames follow the close (the frame
        // stream stays open because the pump's registry entry is not yet
        // cleaned up on exit)
        assert!(events.send("after-close".into()).is_err());
        tokio::task::yield_now().await;
        assert!(matches!(
            frames.try_recv(),
            Err(mpsc::error::TryRecvError::Empty)
        ));
    }
}
