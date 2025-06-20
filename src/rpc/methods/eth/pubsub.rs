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

use std::str::FromStr;

use fvm_ipld_blockstore::Blockstore;
use itertools::Itertools;
use tokio::sync::broadcast::{Receiver as Subscriber, error::RecvError};

use crate::rpc::Ctx;
use crate::rpc::eth::EthFilterSpec;
use crate::rpc::eth::types::{EthAddress, EthAddressList};

pub const ETH_SUBSCRIPTION: &str = "eth_subscription";

const NEW_HEADS: &str = "newHeads";
const PENDING_TXS: &str = "pendingTransactions";
const LOGS: &str = "logs";

pub async fn eth_subscribe<DB: Blockstore + Sync + Send + 'static>(
    params: jsonrpsee::types::Params<'static>,
    pending: jsonrpsee::core::server::PendingSubscriptionSink,
    ctx: Ctx<DB>,
    _ext: http::Extensions,
) -> impl jsonrpsee::IntoSubscriptionCloseResponse {
    let (first, event_types) = match params.parse::<Vec<String>>() {
        Ok(v) => {
            if let Some(event) = v.first() {
                match event.as_str() {
                    NEW_HEADS | PENDING_TXS | LOGS => (event.to_string(), v),
                    _ => {
                        pending
                            .reject(jsonrpsee::types::ErrorObjectOwned::owned(
                                1,
                                format!("unsupported event type: {}", event),
                                None::<String>,
                            ))
                            .await;
                        return Ok(());
                    }
                }
            } else {
                pending
                    .reject(jsonrpsee::types::ErrorObjectOwned::owned(
                        1,
                        "decoding params: expected 1 or 2 params, got 0".to_string(),
                        None::<String>,
                    ))
                    .await;
                return Ok(());
            }
        }
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
    // `event_types` is one of:
    //  - "newHeads": notify when new blocks arrive
    //  - "pendingTransactions": notify when new messages arrive in the message pool
    //  - "logs": notify new event logs that match a criteria
    tracing::trace!("Subscribing to events: [{}]", event_types.iter().join(","));

    match (first.as_str(), event_types.get(1)) {
        (NEW_HEADS, None) => {
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
        (LOGS, _filter) => {
            let spec = EthFilterSpec {
                address: EthAddressList::Single(
                    EthAddress::from_str("0x6c3f61ba9b4abe943bb61bf1f28b79e3f8018b0e").unwrap(),
                ),
                ..Default::default()
            };

            // Spawn logs task
            let logs = crate::rpc::chain::logs(&ctx, Some(spec));

            tokio::spawn(async move {
                // Mark the subscription is accepted after the params has been parsed successful.
                // This is actually responds the underlying RPC method call and may fail if the
                // connection is closed.
                let sink = pending.accept().await.unwrap();

                tracing::trace!(
                    "Subscription task started (id: {:?})",
                    sink.subscription_id()
                );

                handle_subscription(logs, sink).await;
            });
        }
        _ => (),
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

// fn pending_txs<DB: Blockstore + Sync + Send + 'static>(
//     ctx: &Ctx<DB>,
// ) -> Subscriber<Vec<SignedMessage>> {
//     let (sender, receiver) = broadcast::channel(100);

//     let mut subscriber = ctx.mpool.api.subscribe_head_changes();

//     let task_mpool = ctx.mpool.clone();

//     tokio::spawn(async move {
//         while let Ok(v) = subscriber.recv().await {
//             let messages = match v {
//                 HeadChange::Apply(_) => {
//                     let local_msgs = task_mpool.local_msgs.write();
//                     let pending = local_msgs.iter().cloned().collect::<Vec<SignedMessage>>();
//                     pending
//                 }
//             };

//             if sender.send(messages).is_err() {
//                 break;
//             }
//         }
//     });

//     receiver
// }
