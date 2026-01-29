// Copyright 2019-2026 ChainSafe Systems
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

use crate::rpc::eth::pubsub_trait::{
    EthPubSubApiServer, LogFilter, SubscriptionKind, SubscriptionParams,
};
use crate::rpc::{RPCState, chain};
use fvm_ipld_blockstore::Blockstore;
use jsonrpsee::PendingSubscriptionSink;
use jsonrpsee::core::{SubscriptionError, SubscriptionResult};
use std::sync::Arc;
use tokio::sync::broadcast::{Receiver as Subscriber, error::RecvError};

#[derive(derive_more::Constructor)]
pub struct EthPubSub<DB> {
    ctx: Arc<RPCState<DB>>,
}

#[async_trait::async_trait]
impl<DB> EthPubSubApiServer for EthPubSub<DB>
where
    DB: Blockstore + Send + Sync + 'static,
{
    async fn subscribe(
        &self,
        pending: PendingSubscriptionSink,
        kind: SubscriptionKind,
        params: Option<SubscriptionParams>,
    ) -> SubscriptionResult {
        let sink = pending.accept().await?;
        let ctx = self.ctx.clone();

        match kind {
            SubscriptionKind::NewHeads => self.handle_new_heads_subscription(sink, ctx).await,
            SubscriptionKind::PendingTransactions => {
                return Err(SubscriptionError::from(
                    jsonrpsee::types::ErrorObjectOwned::owned(
                        jsonrpsee::types::error::METHOD_NOT_FOUND_CODE,
                        "pendingTransactions subscription not yet implemented",
                        None::<()>,
                    ),
                ));
            }
            SubscriptionKind::Logs => {
                let filter = params.and_then(|p| p.filter);
                self.handle_logs_subscription(sink, ctx, filter).await
            }
        }

        Ok(())
    }
}

impl<DB> EthPubSub<DB>
where
    DB: Blockstore + Send + Sync + 'static,
{
    async fn handle_new_heads_subscription(
        &self,
        accepted_sink: jsonrpsee::SubscriptionSink,
        ctx: Arc<RPCState<DB>>,
    ) {
        let (subscriber, handle) = chain::new_heads(ctx);
        tokio::spawn(async move {
            handle_subscription(subscriber, accepted_sink, handle).await;
        });
    }

    async fn handle_logs_subscription(
        &self,
        accepted_sink: jsonrpsee::SubscriptionSink,
        ctx: Arc<RPCState<DB>>,
        filter_spec: Option<LogFilter>,
    ) {
        let filter_spec = filter_spec.map(Into::into);
        let (logs, handle) = chain::logs(&ctx, filter_spec);
        tokio::spawn(async move {
            handle_subscription(logs, accepted_sink, handle).await;
        });
    }
}

async fn handle_subscription<T>(
    mut subscriber: Subscriber<T>,
    sink: jsonrpsee::SubscriptionSink,
    handle: tokio::task::JoinHandle<()>,
) where
    T: serde::Serialize + Clone,
{
    loop {
        tokio::select! {
            action = subscriber.recv() => {
                match action {
                    Ok(v) => {
                        match jsonrpsee::SubscriptionMessage::new(sink.method_name(), sink.subscription_id(), &v) {
                            Ok(msg) => {
                                if let Err(e) = sink.send(msg).await {
                                    tracing::error!("Failed to send message: {:?}", e);
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
    handle.abort();

    tracing::info!("Subscription task ended (id: {:?})", sink.subscription_id());
}
