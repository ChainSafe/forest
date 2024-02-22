// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
//! Subscription related types and traits for server implementations.
//!
//! Most of the code in this module comes from the `jsonrpsee` crate.
//! See <https://github.com/paritytech/jsonrpsee/blob/v0.21.0/core/src/server/subscription.rs>.
//! We slightly customized it from the original design to support Filecoin `pubsub` specification.
//! The main types that have changed are the `PendingSubscriptionSink` and `SubscriptionSink`.
//! The remaining types and methods must be duplicated because they are private.

use jsonrpsee::core::server::error::{
    DisconnectError, PendingSubscriptionAcceptError, SendTimeoutError, TrySendError,
};
use jsonrpsee::core::server::helpers::{MethodResponse, MethodSink};
use jsonrpsee::helpers::MethodResponseResult;
use jsonrpsee::server::{SubscriptionMessage, SubscriptionMessageInner, SubscriptionPermit};
use jsonrpsee::types::{ErrorObjectOwned, Id, ResponsePayload, SubscriptionId};

use parking_lot::Mutex;
use rustc_hash::FxHashMap;
use std::{sync::Arc, time::Duration};
use tokio::sync::{mpsc, oneshot, OwnedSemaphorePermit};

pub const NOTIF_METHOD_NAME: &str = "xrpc.ch.val";
pub const CANCEL_METHOD_NAME: &str = "xrpc.cancel";
pub const CLOSE_METHOD_NAME: &str = "xrpc.ch.close";

/// Type-alias for subscribers.
pub type Subscribers = Arc<
    Mutex<FxHashMap<SubscriptionKey, (MethodSink, mpsc::Receiver<()>, SubscriptionId<'static>)>>,
>;

/// Represent a unique subscription entry based on [`SubscriptionId`] and [`ConnectionId`].
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SubscriptionKey {
    pub(crate) sub_id: SubscriptionId<'static>,
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
    /// Unique subscription.
    pub(crate) uniq_sub: SubscriptionKey,
    /// ID of the `subscription call` (i.e. not the same as subscription id) which is used
    /// to reply to subscription method call and must only be used once.
    pub(crate) id: Id<'static>,
    /// Sender to answer the subscribe call.
    pub(crate) subscribe: oneshot::Sender<MethodResponse>,
    /// Subscription permit.
    pub(crate) permit: OwnedSemaphorePermit,
    /// Needed by Filecoin `pubsub` specification.
    pub(crate) channel_id: SubscriptionId<'static>,
}

impl PendingSubscriptionSink {
    /// Reject the subscription by responding to the subscription method call with
    /// the error message from [`jsonrpsee_types::error::ErrorObject`].
    ///
    /// # Note
    ///
    /// If this is used in the async subscription callback
    /// the return value is simply ignored because no further notification are propagated
    /// once reject has been called.
    pub async fn reject(self, err: impl Into<ErrorObjectOwned>) {
        let err = MethodResponse::subscription_error(self.id, err.into());
        _ = self.inner.send(err.result.clone()).await;
        _ = self.subscribe.send(err);
    }

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
            let (tx, rx) = mpsc::channel(1);
            self.subscribers.lock().insert(
                self.uniq_sub.clone(),
                (self.inner.clone(), rx, self.channel_id.clone()),
            );
            Ok(SubscriptionSink {
                inner: self.inner,
                method: self.method,
                subscribers: self.subscribers,
                uniq_sub: self.uniq_sub,
                _permit: Arc::new(self.permit),
                channel_id: self.channel_id.clone(),
            })
        } else {
            panic!("The subscription response was too big; adjust the `max_response_size` or change Subscription ID generation");
        }
    }

    /// Returns the channel identifier
    pub fn channel_id<'a>(&self) -> SubscriptionId<'a> {
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
    /// Unique subscription.
    uniq_sub: SubscriptionKey,
    /// Subscription permit.
    _permit: Arc<SubscriptionPermit>,
    /// Channel ID.
    channel_id: SubscriptionId<'static>,
}

impl SubscriptionSink {
    /// Get the subscription ID.
    pub fn subscription_id(&self) -> SubscriptionId<'static> {
        self.uniq_sub.sub_id.clone()
    }

    /// Get the method name.
    pub fn method_name(&self) -> &str {
        self.method
    }

    /// Get the channel ID.
    pub fn channel_id(&self) -> SubscriptionId<'static> {
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
            &self.subscription_id(),
            self.method,
        );
        self.inner.send(json).await.map_err(Into::into)
    }

    /// Similar to to `SubscriptionSink::send` but only waits for a limited time.
    pub async fn send_timeout(
        &self,
        msg: SubscriptionMessage,
        timeout: Duration,
    ) -> Result<(), SendTimeoutError> {
        // Only possible to trigger when the connection is dropped.
        if self.is_closed() {
            return Err(SendTimeoutError::Closed(msg));
        }

        let json = sub_message_to_json(
            msg,
            SubNotifResultOrError::Result,
            &self.subscription_id(),
            self.method,
        );
        self.inner
            .send_timeout(json, timeout)
            .await
            .map_err(Into::into)
    }

    /// Attempts to immediately send out the message as JSON string to the subscribers but fails if the
    /// channel is full or the connection/subscription is closed
    ///
    ///
    /// This differs from [`SubscriptionSink::send`] where it will until there is capacity
    /// in the channel.
    pub fn try_send(&mut self, msg: SubscriptionMessage) -> Result<(), TrySendError> {
        // Only possible to trigger when the connection is dropped.
        if self.is_closed() {
            return Err(TrySendError::Closed(msg));
        }

        let json = sub_message_to_json(
            msg,
            SubNotifResultOrError::Result,
            &self.uniq_sub.sub_id,
            self.method,
        );
        self.inner.try_send(json).map_err(Into::into)
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
        self.subscribers.lock().remove(&self.uniq_sub);
    }
}

pub(crate) fn sub_message_to_json(
    msg: SubscriptionMessage,
    result_or_err: SubNotifResultOrError,
    sub_id: &SubscriptionId,
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
pub fn close_channel_response(channel_id: SubscriptionId) -> MethodResponse {
    let channel_id =
        serde_json::to_string(&channel_id).expect("JSON serialization infallible; qed");
    let msg =
        format!(r#"{{"jsonrpc":"2.0","method":"{CLOSE_METHOD_NAME}","params":[{channel_id}]}}"#,);
    MethodResponse {
        result: msg,
        success_or_error: MethodResponseResult::Success,
        is_subscription: false,
    }
}
