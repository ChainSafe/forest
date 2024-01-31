// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use jsonrpsee::core::server::error::{
    DisconnectError, PendingSubscriptionAcceptError, SendTimeoutError, TrySendError,
};
use jsonrpsee::core::server::helpers::{MethodResponse, MethodSink};
use jsonrpsee::core::server::SubscriptionMessage;
use jsonrpsee::server::SubscriptionMessageInner;
use jsonrpsee::types::{ErrorObjectOwned, Id, ResponsePayload, SubscriptionId};

use parking_lot::Mutex;
use rustc_hash::FxHashMap;
use std::{sync::Arc, time::Duration};
use tokio::sync::{mpsc, oneshot, OwnedSemaphorePermit};

/// Connection ID, used for stateful protocol such as WebSockets.
/// For stateless protocols such as http it's unused, so feel free to set it some hardcoded value.
pub type ConnectionId = usize;

/// Type-alias for subscribers.
pub type Subscribers = Arc<
    Mutex<
        FxHashMap<
            SubscriptionKey,
            (
                MethodSink,
                mpsc::Receiver<()>,
                Option<SubscriptionId<'static>>,
            ),
        >,
    >,
>;
/// Subscription permit.
pub type SubscriptionPermit = OwnedSemaphorePermit;

/// Represents what action that will sent when a subscription callback returns.
#[derive(Debug)]
pub enum SubscriptionCloseResponse {
    /// No further message will be sent.
    None,
    /// Send a subscription notification.
    ///
    /// The subscription notification has the following format:
    ///
    /// ```json
    /// {
    ///  "jsonrpc": "2.0",
    ///  "method": "<method>",
    ///  "params": {
    ///    "subscription": "<subscriptionID>",
    ///    "result": <your msg>
    ///    }
    ///  }
    /// }
    /// ```
    Notif(SubscriptionMessage),
    /// Send a subscription error notification
    ///
    /// The error notification has the following format:
    ///
    /// ```json
    /// {
    ///  "jsonrpc": "2.0",
    ///  "method": "<method>",
    ///  "params": {
    ///    "subscription": "<subscriptionID>",
    ///    "error": <your msg>
    ///    }
    ///  }
    /// }
    /// ```
    NotifErr(SubscriptionMessage),
}

/// Represent a unique subscription entry based on [`SubscriptionId`] and [`ConnectionId`].
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SubscriptionKey {
    pub(crate) conn_id: ConnectionId,
    pub(crate) sub_id: SubscriptionId<'static>,
}

#[derive(Debug, Clone, Copy)]
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

/// Represents a subscription until it is unsubscribed.
///
// NOTE: The reason why we use `mpsc` here is because it allows `IsUnsubscribed::unsubscribed`
// to be &self instead of &mut self.
#[derive(Debug, Clone)]
pub struct IsUnsubscribed(mpsc::Sender<()>);

impl IsUnsubscribed {
    /// Returns true if the unsubscribe method has been invoked or the subscription has been canceled.
    ///
    /// This can be called multiple times as the element in the channel is never
    /// removed.
    pub fn is_unsubscribed(&self) -> bool {
        self.0.is_closed()
    }

    /// Wrapper over [`tokio::sync::mpsc::Sender::closed`]
    ///
    /// # Cancel safety
    ///
    /// This method is cancel safe. Once the channel is closed,
    /// it stays closed forever and all future calls to closed will return immediately.
    pub async fn unsubscribed(&self) {
        self.0.closed().await;
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
pub struct ForestPendingSubscriptionSink {
    /// Sink.
    pub inner: MethodSink,
    /// MethodCallback.
    pub method: &'static str,
    /// Shared Mutex of subscriptions for this method.
    pub subscribers: Subscribers,
    /// Unique subscription.
    pub uniq_sub: SubscriptionKey,
    /// ID of the `subscription call` (i.e. not the same as subscription id) which is used
    /// to reply to subscription method call and must only be used once.
    pub id: Id<'static>,
    /// Sender to answer the subscribe call.
    pub subscribe: oneshot::Sender<MethodResponse>,
    /// Subscription permit.
    pub permit: OwnedSemaphorePermit,

    /// For Filecoin `pubsub`
    pub channel_id: Option<SubscriptionId<'static>>,
}

impl ForestPendingSubscriptionSink {
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
        let sub_id = self.subscription_id();
        let response = MethodResponse::subscription_response(
            self.id,
            ResponsePayload::result_borrowed(&sub_id),
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
                unsubscribe: IsUnsubscribed(tx),
                _permit: Arc::new(self.permit),
                channel_id: self.channel_id.clone(),
            })
        } else {
            panic!("The subscription response was too big; adjust the `max_response_size` or change Subscription ID generation");
        }
    }

    /// Returns connection identifier, which was used to perform pending subscription request
    pub fn connection_id(&self) -> ConnectionId {
        self.uniq_sub.conn_id
    }

    /// Returns the subscription identifier
    pub fn subscription_id<'a>(&self) -> SubscriptionId<'a> {
        // TODO: document
        if let Some(sub_id) = self.channel_id.clone() {
            sub_id
        } else {
            self.uniq_sub.sub_id.clone()
        }
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
    /// A future to that fires once the unsubscribe method has been called.
    unsubscribe: IsUnsubscribed,
    /// Subscription permit
    _permit: Arc<SubscriptionPermit>,

    /// Optional channel ID for Filecoin `pubsub`.
    channel_id: Option<SubscriptionId<'static>>,
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

    /// Get the connection ID.
    pub fn connection_id(&self) -> ConnectionId {
        self.uniq_sub.conn_id
    }

    /// Get the channel ID if some.
    pub fn channel_id(&self) -> Option<SubscriptionId<'static>> {
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
        //todo!()
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
        self.inner.is_closed() || !self.is_active_subscription()
    }

    /// Completes when the subscription has been closed.
    pub async fn closed(&self) {
        // Both are cancel-safe thus ok to use select here.
        tokio::select! {
            _ = self.inner.closed() => (),
            _ = self.unsubscribe.unsubscribed() => (),
        }
    }

    fn is_active_subscription(&self) -> bool {
        !self.unsubscribe.is_unsubscribed()
    }
}

impl Drop for SubscriptionSink {
    fn drop(&mut self) {
        if self.is_active_subscription() {
            self.subscribers.lock().remove(&self.uniq_sub);
        }
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
