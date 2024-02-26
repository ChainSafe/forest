// Copyright 2019-2024 ChainSafe Systems
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
//!         │                          A few moments later                        │
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

use jsonrpsee::core::server::error::{DisconnectError, PendingSubscriptionAcceptError};
use jsonrpsee::core::server::helpers::{MethodResponse, MethodSink};
use jsonrpsee::server::{
    IntoSubscriptionCloseResponse, MethodCallback, Methods, RegisterMethodError,
    SubscriptionMessage, SubscriptionMessageInner,
};
use jsonrpsee::types::{error::ErrorCode, Id, Params, ResponsePayload};
use jsonrpsee::IntoResponse;

use parking_lot::Mutex;
use rustc_hash::FxHashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::{mpsc, oneshot};

use super::error::JsonRpcError;

pub const NOTIF_METHOD_NAME: &str = "xrpc.ch.val";
pub const CANCEL_METHOD_NAME: &str = "xrpc.cancel";
pub const CLOSE_METHOD_NAME: &str = "xrpc.ch.close";

pub type ChannelId = u64;

/// Type-alias for subscribers.
pub type Subscribers =
    Arc<Mutex<FxHashMap<ChannelId, (MethodSink, mpsc::Receiver<()>, ChannelId)>>>;

/// Represent a unique subscription entry based on [`SubscriptionId`] and [`ConnectionId`].
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SubscriptionKey {
    pub(crate) sub_id: ChannelId,
}

#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub(crate) enum SubNotifResultOrError {
    Result,
    Error,
}

impl SubNotifResultOrError {
    pub(crate) const fn as_str(&self) -> &str {
        match self {
            Self::Result => "result",
            Self::Error => "error",
        }
    }
}

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
}

impl PendingSubscriptionSink {
    /// Attempt to accept the subscription and respond the subscription method call.
    ///
    /// # Panics
    ///
    /// Panics if the subscription response exceeded the `max_response_size`.
    pub async fn accept(self) -> Result<SubscriptionSink, PendingSubscriptionAcceptError> {
        let channel_id = self.channel_id();
        let response = MethodResponse::subscription_response(
            self.id,
            ResponsePayload::result_borrowed(&channel_id),
            self.inner.max_response_size() as usize,
        );
        let success = response.is_success();

        // TODO: #1052
        //
        // Ideally the message should be sent only once.
        //
        // The same message is sent twice here because one is sent directly to the transport layer and
        // the other one is sent internally to accept the subscription.
        self.inner
            .send(response.result.clone())
            .await
            .map_err(|_| PendingSubscriptionAcceptError)?;
        self.subscribe
            .send(response)
            .map_err(|_| PendingSubscriptionAcceptError)?;

        if success {
            let (_tx, rx) = mpsc::channel(1);
            self.subscribers.lock().insert(
                self.channel_id.clone(),
                (self.inner.clone(), rx, self.channel_id.clone()),
            );
            Ok(SubscriptionSink {
                inner: self.inner,
                method: self.method,
                subscribers: self.subscribers,
                channel_id: self.channel_id.clone(),
            })
        } else {
            panic!("The subscription response was too big; adjust the `max_response_size` or change Subscription ID generation");
        }
    }

    /// Returns the channel identifier
    pub fn channel_id(&self) -> ChannelId {
        self.channel_id.clone()
    }
}

/// Represents a single subscription that hasn't been processed yet.
#[derive(Debug, Clone)]
pub struct SubscriptionSink {
    /// Sink.
    inner: MethodSink,
    /// MethodCallback.
    method: &'static str,
    /// Shared Mutex of subscriptions for this method.
    subscribers: Subscribers,
    /// Channel identifier.
    channel_id: ChannelId,
}

impl SubscriptionSink {
    // /// Get the subscription ID.
    // pub fn subscription_id(&self) -> SubscriptionId<'static> {
    //     self.uniq_sub.sub_id.clone()
    // }

    /// Get the method name.
    pub fn method_name(&self) -> &str {
        self.method
    }

    /// Get the channel ID.
    pub fn channel_id(&self) -> ChannelId {
        self.channel_id.clone()
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
    pub async fn send(&self, msg: SubscriptionMessage) -> Result<(), DisconnectError> {
        // Only possible to trigger when the connection is dropped.
        if self.is_closed() {
            return Err(DisconnectError(msg));
        }

        let json = sub_message_to_json(
            msg,
            SubNotifResultOrError::Result,
            self.channel_id(),
            self.method,
        );
        self.inner.send(json).await.map_err(Into::into)
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
        }
    }
}

impl Drop for SubscriptionSink {
    fn drop(&mut self) {
        self.subscribers.lock().remove(&self.channel_id);
    }
}

pub(crate) fn sub_message_to_json(
    msg: SubscriptionMessage,
    result_or_err: SubNotifResultOrError,
    sub_id: ChannelId,
    method: &str,
) -> String {
    let result_or_err = result_or_err.as_str();

    match msg.0 {
        SubscriptionMessageInner::Complete(msg) => msg,
        SubscriptionMessageInner::NeedsData(result) => {
            let sub_id = serde_json::to_string(&sub_id).expect("valid JSON; qed");
            format!(
                r#"{{"jsonrpc":"2.0","method":"{method}","params":{{"subscription":{sub_id},"{result_or_err}":{result}}}}}"#,
            )
        }
    }
}

/// Creates a notification message.
#[allow(unused)]
pub fn create_notif_message(
    sink: &SubscriptionSink,
    result: &impl serde::Serialize,
) -> anyhow::Result<SubscriptionMessage> {
    let method = sink.method_name();
    let channel_id =
        serde_json::to_string(&sink.channel_id()).expect("JSON serialization infallible; qed");
    let result = serde_json::to_string(result)?;
    let msg =
        format!(r#"{{"jsonrpc":"2.0","method":"{method}","params":[{channel_id},{result}]}}"#,);

    Ok(SubscriptionMessage::from_complete_message(msg))
}

/// Creates a close channel method response.
pub fn close_channel_message(channel_id: ChannelId) -> SubscriptionMessage {
    let channel_id =
        serde_json::to_string(&channel_id).expect("JSON serialization infallible; qed");
    let msg =
        format!(r#"{{"jsonrpc":"2.0","method":"{CLOSE_METHOD_NAME}","params":[{channel_id}]}}"#,);
    SubscriptionMessage::from_complete_message(msg)
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

impl RpcModule {
    /// Create a new module with a given shared `Context`.
    pub fn new() -> Self {
        let mut methods = Methods::default();

        let channels = Subscribers::default();
        methods
            .verify_and_insert(
                CANCEL_METHOD_NAME,
                MethodCallback::Sync(Arc::new({
                    let channels = channels.clone();
                    move |id, params, max_response| {
                        tracing::debug!("Got cancel request: {id}");
                        let cb = || {
                            let arr: [ChannelId; 1] = params.parse()?;
                            let channel_id = arr[0];
                            tracing::debug!("Got cancel request: {id} {channel_id}");
                            channels.lock().remove(&channel_id);
                            Ok::<bool, JsonRpcError>(true)
                        };
                        let ret = cb().into_response();
                        MethodResponse::response(id, ret, max_response)
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
                tracing::debug!("Creating channel");

                let mut receiver = callback(params);
                tokio::spawn(async move {
                    let sink = pending.accept().await.unwrap();

                    loop {
                        tokio::select! {
                            action = receiver.recv() => {
                                match action {
                                    Ok(msg) => {
                                        if let Ok(msg) = create_notif_message(&sink, &msg) {
                                            // This fails only if the connection is closed
                                            if let Ok(()) = sink.send(msg).await {
                                            } else {
                                                break;
                                            }
                                        } else {
                                            break;
                                        }
                                    }
                                    Err(RecvError::Closed) => {
                                        let msg = close_channel_message(sink.channel_id());
                                        // This fails only if the connection is closed
                                        if let Ok(()) = sink.send(msg).await {
                                        } else {
                                            break;
                                        }
                                    }
                                    Err(RecvError::Lagged(_)) => {
                                    }
                                }
                            },
                            _ = sink.closed() => {
                                break;
                            }
                        }
                    }
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
                move |id, params, method_sink, _conn| {
                    let channel_id = id_provider.fetch_add(1, Ordering::Relaxed);

                    // response to the subscription call.
                    let (tx, rx) = oneshot::channel();

                    let sink = PendingSubscriptionSink {
                        inner: method_sink.clone(),
                        method: NOTIF_METHOD_NAME,
                        subscribers: subscribers.clone(),
                        id: id.clone().into_owned(),
                        subscribe: tx,
                        channel_id,
                    };

                    callback(params, sink);

                    let id = id.clone().into_owned();

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
