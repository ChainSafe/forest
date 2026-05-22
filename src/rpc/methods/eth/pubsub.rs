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

use crate::blocks::Tipset;
use crate::message_pool::MpoolUpdate;
use crate::prelude::ShallowClone;
use crate::rpc::RPCState;
use crate::rpc::eth::pubsub_trait::{EthPubSubApiServer, SubscriptionKind, SubscriptionParams};
use crate::rpc::eth::types::{ApiHeaders, EthFilterSpec};
use crate::rpc::eth::{
    Block as EthBlock, TxInfo, eth_logs_with_filter, eth_tx_hash_from_signed_message,
};
use crate::utils::broadcast::subscription_stream;
use futures::{Stream, StreamExt as _};
use jsonrpsee::core::SubscriptionResult;
use jsonrpsee::{PendingSubscriptionSink, SubscriptionSink};
use std::sync::Arc;

#[derive(derive_more::Constructor)]
pub struct EthPubSub {
    ctx: Arc<RPCState>,
}

#[async_trait::async_trait]
impl EthPubSubApiServer for EthPubSub {
    async fn subscribe(
        &self,
        pending: PendingSubscriptionSink,
        kind: SubscriptionKind,
        params: Option<SubscriptionParams>,
    ) -> SubscriptionResult {
        let sink = pending.accept().await?;
        let ctx = self.ctx.shallow_clone();
        match kind {
            SubscriptionKind::NewHeads => spawn_new_heads(sink, ctx),
            SubscriptionKind::PendingTransactions => spawn_pending_transactions(sink, ctx),
            SubscriptionKind::Logs => {
                let filter = params.and_then(|p| p.filter).map(EthFilterSpec::from);
                spawn_logs(sink, ctx, filter);
            }
        }

        Ok(())
    }
}

/// Stream of tipsets as they are applied to the chain head. Reverts are
/// ignored; lagged events are dropped (and logged) by [`subscription_stream`].
fn head_applied_tipsets(ctx: &Arc<RPCState>) -> impl Stream<Item = Tipset> + Send + use<> {
    subscription_stream(ctx.chain_store().subscribe_head_changes())
        .flat_map(|changes| futures::stream::iter(changes.applies))
}

fn spawn_new_heads(sink: SubscriptionSink, ctx: Arc<RPCState>) {
    let stream = head_applied_tipsets(&ctx)
        .filter_map(move |ts| {
            let state_mngr = ctx.state_manager.shallow_clone();
            async move {
                EthBlock::from_filecoin_tipset(&state_mngr, ts, TxInfo::Full)
                    .await
                    .inspect_err(|e| {
                        tracing::error!("Failed to convert tipset to eth block: {e:#}")
                    })
                    .ok()
                    .map(ApiHeaders)
            }
        })
        .boxed();
    tokio::spawn(pipe_stream_to_sink(stream, sink));
}

fn spawn_logs(sink: SubscriptionSink, ctx: Arc<RPCState>, filter: Option<EthFilterSpec>) {
    let stream = head_applied_tipsets(&ctx)
        .filter_map(move |ts| {
            let ctx = ctx.shallow_clone();
            let filter = filter.clone();
            async move {
                eth_logs_with_filter(&ctx, &ts, filter)
                    .await
                    .inspect_err(|e| {
                        tracing::error!("Failed to fetch logs for tipset {}: {e:#}", ts.key())
                    })
                    .ok()
                    // Skip tipsets with no matching logs — nothing to notify.
                    .filter(|logs| !logs.is_empty())
            }
        })
        .boxed();
    tokio::spawn(pipe_stream_to_sink(stream, sink));
}

fn spawn_pending_transactions(sink: SubscriptionSink, ctx: Arc<RPCState>) {
    let mpool_rx = ctx.mpool.subscribe_to_updates();
    let eth_chain_id = ctx.chain_config().eth_chain_id;
    let stream = subscription_stream(mpool_rx)
        .filter_map(move |update| async move {
            let MpoolUpdate::Add(msg) = update else {
                return None;
            };
            eth_tx_hash_from_signed_message(&msg, eth_chain_id).ok()
        })
        .boxed();
    tokio::spawn(pipe_stream_to_sink(stream, sink));
}

/// Forward stream items to the subscription sink until the sink is closed,
/// the client disconnects, or the upstream stream ends. The stream is
/// expected to absorb upstream backpressure (e.g. `Lagged`) on its own; this
/// helper only cares about the sink side.
async fn pipe_stream_to_sink<S, T>(mut stream: S, sink: SubscriptionSink)
where
    S: Stream<Item = T> + Unpin + Send,
    T: serde::Serialize + Send,
{
    loop {
        tokio::select! {
            _ = sink.closed() => break,
            maybe = stream.next() => {
                let Some(item) = maybe else { break };
                let msg = match jsonrpsee::SubscriptionMessage::new(
                    sink.method_name(),
                    sink.subscription_id(),
                    &item,
                ) {
                    Ok(m) => m,
                    Err(e) => {
                        tracing::error!("Failed to serialize subscription message: {e:?}");
                        break;
                    }
                };
                if let Err(e) = sink.send(msg).await {
                    tracing::debug!("Subscription sink send failed (client disconnected): {e:?}");
                    break;
                }
            }
        }
    }
    tracing::debug!("Subscription task ended (id: {:?})", sink.subscription_id());
}
