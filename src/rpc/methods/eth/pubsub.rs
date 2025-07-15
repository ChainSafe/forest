// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Official documentation for the Ethereum pubsub protocol is available at:
//! https://geth.ethereum.org/docs/interacting-with-geth/rpc/pubsub
//!
//! Note that Filecoin uses this protocol without modifications.
//!
//! The sequence diagram for an event subscription is shown below:
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
//!         │  │  method:'eth_subscribe',       │                                 │
//!         │  │  params:[<eventType>] }        │                                 │
//!         │  └────────────────────────────────┘                                 │
//!         │                                 ┌────────────────────────────────┐  │
//!         │ ◀───────────────────────────────┤ Opened subscription message    ├──│
//!         │                                 │                                │  │
//!         │                                 │{ jsonrpc:'2.0',                │  │
//!         │                                 │  id:<id>,                      │  │
//!         │                                 │  result:<subId> }              │  │
//!         │                                 └────────────────────────────────┘  │
//!         │                                                                     │
//!         │                                                                     │
//!         │                                 ┌────────────────────────────────┐  │
//!         │ ◀───────────────────────────────┤ Notification message           ├──│
//!         │                                 │                                │  │
//!         │                                 │{ jsonrpc:'2.0',                │  │
//!         │                                 │  method:'eth_subscription',    │  │
//!         │                                 │  params:{ subscription:<subId>,│  │
//!         │                                 │           result:<payload> } } │  │
//!         │                                 └────────────────────────────────┘  │
//!         │                                                                     │
//!         │                                                                     │
//!         │                                                                     │
//!         │                      After a few notifications                      │
//!         │  ┌────────────────────────────────┐                                 │
//!         │──┤ Cancel subscription            ├───────────────────────────────▶ │
//!         │  │                                │                                 │
//!         │  │{ jsonrpc:'2.0',                │                                 │
//!         │  │  id:<id>,                      │                                 │
//!         │  │  method:'eth_unsubscribe',     │                                 │
//!         │  │  params:[<subId>] }            │                                 │
//!         │  └────────────────────────────────┘                                 │
//!         │                                 ┌────────────────────────────────┐  │
//!         │ ◀───────────────────────────────┤ Closed subscription message    ├──│
//!         │                                 │                                │  │
//!         │                                 │{ jsonrpc:'2.0',                │  │
//!         │                                 │  id:<id>,                      │  │
//!         │                                 │  result:true }                 │  │
//!         │                                 └────────────────────────────────┘  │
//! ```
//!

use std::fmt;

use fvm_ipld_blockstore::Blockstore;
use serde::de::{self, Deserializer, SeqAccess, Visitor};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast::{Receiver as Subscriber, error::RecvError};

use crate::rpc::Ctx;
use crate::rpc::eth::types::EthAddressList;
use crate::rpc::eth::{EthFilterSpec, EthTopicSpec};

pub const ETH_SUBSCRIPTION: &str = "eth_subscription";

const NEW_HEADS: &str = "newHeads";
const PENDING_TRANSACTIONS: &str = "pendingTransactions";
const LOGS: &str = "logs";

#[derive(Default, Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct LogFilter {
    pub address: EthAddressList,
    pub topics: Option<EthTopicSpec>,
}

#[derive(Debug)]
enum Subscription {
    NewHeads,
    PendingTransactions,
    Logs(Option<LogFilter>),
}

impl<'de> Deserialize<'de> for Subscription {
    fn deserialize<D>(deserializer: D) -> Result<Subscription, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct SubscriptionVisitor;

        impl<'de> Visitor<'de> for SubscriptionVisitor {
            type Value = Subscription;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str(r#"a JSON array like ["logs", {...}] or ["newHeads"]"#)
            }

            fn visit_seq<V>(self, mut seq: V) -> Result<Subscription, V::Error>
            where
                V: SeqAccess<'de>,
            {
                let event_type: String = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(0, &self))?;

                match event_type.as_str() {
                    NEW_HEADS => {
                        if seq.next_element::<serde::de::IgnoredAny>()?.is_some() {
                            return Err(de::Error::custom("unsupported event type"));
                        }
                        Ok(Subscription::NewHeads)
                    }
                    PENDING_TRANSACTIONS => {
                        if seq.next_element::<serde::de::IgnoredAny>()?.is_some() {
                            return Err(de::Error::custom("unsupported event type"));
                        }
                        Ok(Subscription::PendingTransactions)
                    }
                    LOGS => Ok(Subscription::Logs(seq.next_element()?)),
                    _ => Err(de::Error::unknown_variant(
                        &event_type,
                        &[NEW_HEADS, PENDING_TRANSACTIONS, LOGS],
                    )),
                }
            }
        }

        deserializer.deserialize_seq(SubscriptionVisitor)
    }
}

pub async fn eth_subscribe<DB: Blockstore + Sync + Send + 'static>(
    params: jsonrpsee::types::Params<'static>,
    pending: jsonrpsee::core::server::PendingSubscriptionSink,
    ctx: Ctx<DB>,
    _ext: http::Extensions,
) -> impl jsonrpsee::IntoSubscriptionCloseResponse {
    let subscription: Subscription = match params.parse() {
        Ok(sub) => sub,
        Err(e) => {
            pending
                .reject(jsonrpsee::types::ErrorObjectOwned::from(e))
                .await;
            // If the subscription has not been "accepted" then
            // the return value will be "ignored" as it's not
            // allowed to send out any further notifications on
            // on the subscription.
            return Ok(());
        }
    };

    tracing::trace!("Subscribing to event: {:?}", subscription);

    match subscription {
        Subscription::NewHeads => {
            // Spawn newHeads task
            let new_heads = crate::rpc::new_heads(&ctx);

            tokio::spawn(async move {
                // Mark the subscription is accepted after the params has been parsed successful.
                // This is actually responds the underlying RPC method call and may fail if the
                // connection is closed.
                let sink = pending.accept().await.unwrap();

                tracing::trace!(
                    "Subscription task started (id: {:?})",
                    sink.subscription_id()
                );

                handle_subscription(new_heads, sink).await;
            });
        }
        Subscription::Logs(filter) => {
            let filter_spec: Option<EthFilterSpec> = filter.map(Into::into);

            // Spawn logs task
            let logs = crate::rpc::chain::logs(&ctx, filter_spec);

            tokio::spawn(async move {
                // Mark the subscription is accepted after the params has been parsed successful.
                // This is actually responds the underlying RPC method call and may fail if the
                // connection is closed.
                let sink = pending.accept().await.unwrap();

                tracing::trace!(
                    "Logs subscription task started (id: {:?})",
                    sink.subscription_id()
                );

                handle_subscription(logs, sink).await;
            });
        }
        Subscription::PendingTransactions => {
            // TODO(akaladarshi): https://github.com/ChainSafe/forest/pull/5782
        }
    }

    Ok(())
}

async fn handle_subscription<T>(mut subscriber: Subscriber<T>, sink: jsonrpsee::SubscriptionSink)
where
    T: serde::Serialize + Clone,
{
    loop {
        tokio::select! {
            action = subscriber.recv() => {
                match action {
                    Ok(v) => {
                        match jsonrpsee::SubscriptionMessage::new("eth_subscription", sink.subscription_id(), &v) {
                            Ok(msg) => {
                                // This fails only if the connection is closed
                                if sink.send(msg).await.is_err() {
                                    break;
                                }
                            }
                            Err(e) => {
                                tracing::error!("Failed to serialize message: {:?}", e);
                                break;
                            }
                        }
                    }
                    Err(RecvError::Closed) => {
                        break;
                    }
                    Err(RecvError::Lagged(_)) => {
                    }
                }
            }
            _ = sink.closed() => {
                break;
            }
        }
    }

    tracing::trace!("Subscription task ended (id: {:?})", sink.subscription_id());
}
