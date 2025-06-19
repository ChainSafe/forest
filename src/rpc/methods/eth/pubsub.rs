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

use crate::rpc::Ctx;
use fvm_ipld_blockstore::Blockstore;
use itertools::Itertools;
use tokio::sync::broadcast::error::RecvError;

pub const ETH_SUBSCRIPTION: &str = "eth_subscription";

pub async fn eth_subscribe<DB: Blockstore>(
    params: jsonrpsee::types::Params<'static>,
    pending: jsonrpsee::core::server::PendingSubscriptionSink,
    ctx: Ctx<DB>,
    _ext: http::Extensions,
) -> impl jsonrpsee::IntoSubscriptionCloseResponse {
    let event_types = match params.parse::<Vec<String>>() {
        Ok(v) => {
            if let Some(event) = v.first() {
                if event != "newHeads" {
                    pending
                        .reject(jsonrpsee::types::ErrorObjectOwned::owned(
                            1,
                            format!("unsupported event type: {}", event),
                            None::<String>,
                        ))
                        .await;
                    return Ok(());
                }
            } else {
                pending
                    .reject(jsonrpsee::types::ErrorObjectOwned::owned(
                        1,
                        "decoding params: expected 1 param, got 0".to_string(),
                        None::<String>,
                    ))
                    .await;
                return Ok(());
            }
            v
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
    // `event_types` is one OR more of:
    //  - "newHeads": notify when new blocks arrive
    //  - "pendingTransactions": notify when new messages arrive in the message pool
    //  - "logs": notify new event logs that match a criteria
    tracing::trace!("Subscribing to events: [{}]", event_types.iter().join(","));

    let mut receiver = crate::rpc::new_heads(&ctx);

    tokio::spawn(async move {
        // Mark the subscription is accepted after the params has been parsed successful.
        // This is actually responds the underlying RPC method call and may fail if the
        // connection is closed.
        let sink = pending.accept().await.unwrap();

        tracing::trace!(
            "Subscription task started (id: {:?})",
            sink.subscription_id()
        );

        loop {
            tokio::select! {
                action = receiver.recv() => {
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
    });

    Ok(())
}
