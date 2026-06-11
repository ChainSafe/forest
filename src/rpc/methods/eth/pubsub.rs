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
use crate::chain::HeadChanges;
use crate::message_pool::MpoolUpdate;
use crate::prelude::ShallowClone;
use crate::rpc::RPCState;
use crate::rpc::chain::PathChange;
use crate::rpc::eth::filter::EventRevertStatus;
use crate::rpc::eth::pubsub_trait::{EthPubSubApiServer, SubscriptionKind, SubscriptionParams};
use crate::rpc::eth::types::{ApiHeaders, EthFilterSpec, EthHashList, EthTopicSpec};
use crate::rpc::eth::{
    Block as EthBlock, EthLog, TxInfo, eth_logs_for_head_change, eth_tx_hash_from_signed_message,
};
use crate::utils::broadcast::subscription_stream;
use futures::{Stream, StreamExt as _};
use jsonrpsee::core::SubscriptionResult;
use jsonrpsee::{PendingSubscriptionSink, SubscriptionSink};
use std::sync::Arc;
use tokio::sync::broadcast;

/// A cap on the number of in-flight per-tipset log batches in the shared logs feed.
const LOGS_FEED_CAP: usize = 256;

/// Sender half of the shared logs feed; see [`RPCState::eth_logs_feed`].
pub type LogsFeed = broadcast::Sender<Arc<Vec<EthLog>>>;

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

/// Stream of "message tipsets", the parent of each newly applied tipset; only used by the
/// `newHeads` subscription. Reverts are ignored; lagged events are dropped (and logged) by
/// [`subscription_stream`].
fn head_message_tipsets(ctx: &Arc<RPCState>) -> impl Stream<Item = Tipset> + Send + use<> {
    let rx = ctx.chain_store().subscribe_head_changes();
    let ctx = ctx.shallow_clone();
    subscription_stream(rx).flat_map(move |changes| {
        let ctx = ctx.shallow_clone();
        let items: Vec<_> = changes
            .applies
            .into_iter()
            .filter_map(|applied| {
                if applied.epoch() == 0 {
                    return None;
                }
                match ctx.chain_index().load_required_tipset(applied.parents()) {
                    Ok(parent) => Some(parent),
                    Err(e) => {
                        tracing::error!("Failed to load parent tipset of {}: {e:#}", applied.key());
                        None
                    }
                }
            })
            .collect();
        futures::stream::iter(items)
    })
}

fn spawn_new_heads(sink: SubscriptionSink, ctx: Arc<RPCState>) {
    let stream = head_message_tipsets(&ctx)
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

fn flatten_head_changes(changes: HeadChanges) -> impl Iterator<Item = (Tipset, EventRevertStatus)> {
    changes
        .into_change_vec()
        .into_iter()
        .map(|change| match change {
            PathChange::Revert(tipset) => (tipset, EventRevertStatus::Reverted),
            PathChange::Apply(tipset) => (tipset, EventRevertStatus::Applied),
        })
}

/// Drives the shared logs feed: for every chain head change, collects the Ethereum logs of
/// the affected tipsets — reorg-reverted ones (marked `removed: true`) before applied ones —
/// and broadcasts each tipset's logs to all live `eth_subscribe("logs")` subscriptions.
async fn run_logs_feed(ctx: Arc<RPCState>, feed: broadcast::Sender<Arc<Vec<EthLog>>>) {
    let mut head_changes = subscription_stream(ctx.chain_store().subscribe_head_changes());
    while let Some(changes) = head_changes.next().await {
        // Collecting events is not free; skip the work entirely while no subscription is live.
        if feed.receiver_count() == 0 {
            continue;
        }
        for (tipset, revert_status) in flatten_head_changes(changes) {
            if tipset.epoch() == 0 {
                continue;
            }
            match eth_logs_for_head_change(&ctx, &tipset, revert_status).await {
                Ok(logs) if !logs.is_empty() => {
                    // An error only means every receiver vanished since the check above.
                    let _ = feed.send(Arc::new(logs));
                }
                Ok(_) => {}
                Err(e) => {
                    tracing::error!(
                        "Failed to collect logs for tipset {} ({revert_status:?}): {e:#}",
                        tipset.key()
                    );
                }
            }
        }
    }
}

fn subscribe_logs_feed(ctx: &Arc<RPCState>) -> broadcast::Receiver<Arc<Vec<EthLog>>> {
    ctx.eth_logs_feed
        .get_or_init(|| {
            let (tx, _) = broadcast::channel(LOGS_FEED_CAP);
            tokio::spawn(run_logs_feed(ctx.clone(), tx.clone()));
            tx
        })
        .subscribe()
}

fn spawn_logs(sink: SubscriptionSink, ctx: Arc<RPCState>, filter: Option<EthFilterSpec>) {
    let rx = subscribe_logs_feed(&ctx);
    let stream = subscription_stream(rx)
        .flat_map(move |logs| {
            let matched: Vec<EthLog> = logs
                .iter()
                .filter(|log| filter.as_ref().is_none_or(|spec| log_matches(spec, log)))
                .cloned()
                .collect();
            futures::stream::iter(matched)
        })
        .boxed();
    tokio::spawn(pipe_stream_to_sink(stream, sink));
}

/// Standard Ethereum log filtering (go-ethereum's `filterLogs`) over an already-converted
/// log: any address in the list may match, with an absent or empty list acting as a
/// wildcard; topic positions are ANDed across positions and ORed within one, with absent or
/// null positions acting as wildcards. A log with fewer topics than the filter has positions
/// never matches. The filter's block range does not apply to subscriptions.
fn log_matches(spec: &EthFilterSpec, log: &EthLog) -> bool {
    let address_matches = spec
        .address
        .as_ref()
        .is_none_or(|addresses| addresses.is_empty() || addresses.contains(&log.address));
    let topics_match = spec.topics.as_ref().is_none_or(|EthTopicSpec(positions)| {
        positions.len() <= log.topics.len()
            && positions
                .iter()
                .zip(&log.topics)
                .all(|(position, topic)| match position {
                    EthHashList::List(hashes) => hashes.is_empty() || hashes.contains(topic),
                    EthHashList::Single(hash) => hash.as_ref().is_none_or(|h| h == topic),
                })
    });
    address_matches && topics_match
}

fn spawn_pending_transactions(sink: SubscriptionSink, ctx: Arc<RPCState>) {
    let mpool_rx = ctx.mpool.subscribe_to_updates();
    let eth_chain_id = ctx.chain_config().eth_chain_id;
    let stream = subscription_stream(mpool_rx)
        .filter_map(move |update| async move {
            let MpoolUpdate::Add(msg) = update else {
                return None;
            };
            eth_tx_hash_from_signed_message(&msg, eth_chain_id)
                .inspect_err(|e| {
                    tracing::error!("Failed to compute eth tx hash from mpool message: {e:#}")
                })
                .ok()
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
                        continue;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blocks::{CachingBlockHeader, RawBlockHeader};
    use crate::rpc::eth::{EthAddress, EthHash};
    use crate::shim::clock::ChainEpoch;
    use std::str::FromStr as _;

    fn tipset(epoch: ChainEpoch) -> Tipset {
        Tipset::from(&CachingBlockHeader::new(RawBlockHeader {
            epoch,
            ..Default::default()
        }))
    }

    #[test]
    fn flatten_head_changes_emits_reverts_before_applies() {
        // `chain_get_path` produces reverts newest-first and applies oldest-first; the
        // flattened order must preserve that and put every revert before any apply.
        let changes = HeadChanges {
            reverts: vec![tipset(5), tipset(4)],
            applies: vec![tipset(14), tipset(15)],
        };
        let flattened: Vec<(ChainEpoch, EventRevertStatus)> = flatten_head_changes(changes)
            .map(|(ts, status)| (ts.epoch(), status))
            .collect();
        assert_eq!(
            flattened,
            vec![
                (5, EventRevertStatus::Reverted),
                (4, EventRevertStatus::Reverted),
                (14, EventRevertStatus::Applied),
                (15, EventRevertStatus::Applied),
            ]
        );
    }

    #[test]
    fn flatten_head_changes_plain_apply() {
        let changes = HeadChanges {
            reverts: vec![],
            applies: vec![tipset(7)],
        };
        let flattened: Vec<(ChainEpoch, EventRevertStatus)> = flatten_head_changes(changes)
            .map(|(ts, status)| (ts.epoch(), status))
            .collect();
        assert_eq!(flattened, vec![(7, EventRevertStatus::Applied)]);
    }

    fn eth_log(address: &EthAddress, topics: Vec<EthHash>) -> EthLog {
        EthLog {
            address: *address,
            topics,
            ..Default::default()
        }
    }

    fn address_0() -> EthAddress {
        EthAddress::from_str("0xff38c072f286e3b20b3954ca9f99c05fbecc64aa").unwrap()
    }

    fn address_1() -> EthAddress {
        EthAddress::from_str("0x26937d59db4463254c930d5f31353f14aa89a0f7").unwrap()
    }

    fn topic(byte: u8) -> EthHash {
        EthHash(ethereum_types::H256::from_slice(&[byte; 32]))
    }

    #[test]
    fn log_matches_address() {
        let log = eth_log(&address_0(), vec![]);

        // Absent and empty address lists are wildcards (Lotus/go-ethereum behavior).
        assert!(log_matches(&EthFilterSpec::default(), &log));
        let empty = EthFilterSpec {
            address: Some(vec![].into()),
            ..Default::default()
        };
        assert!(log_matches(&empty, &log));

        let specific = EthFilterSpec {
            address: Some(vec![address_0()].into()),
            ..Default::default()
        };
        assert!(log_matches(&specific, &log));
        assert!(!log_matches(&specific, &eth_log(&address_1(), vec![])));

        // Any address in the list may match.
        let either = EthFilterSpec {
            address: Some(vec![address_0(), address_1()].into()),
            ..Default::default()
        };
        assert!(log_matches(&either, &log));
        assert!(log_matches(&either, &eth_log(&address_1(), vec![])));
    }

    #[test]
    fn log_matches_topics() {
        let log = eth_log(&address_0(), vec![topic(1), topic(2)]);

        let with_topics = |positions: Vec<EthHashList>| EthFilterSpec {
            topics: Some(EthTopicSpec(positions)),
            ..Default::default()
        };

        // Wildcards: null position, empty list position, fewer positions than topics.
        assert!(log_matches(&with_topics(vec![]), &log));
        assert!(log_matches(
            &with_topics(vec![EthHashList::Single(None)]),
            &log
        ));
        assert!(log_matches(
            &with_topics(vec![EthHashList::List(vec![])]),
            &log
        ));

        // Value in the first position.
        assert!(log_matches(
            &with_topics(vec![EthHashList::Single(Some(topic(1)))]),
            &log
        ));
        assert!(!log_matches(
            &with_topics(vec![EthHashList::Single(Some(topic(2)))]),
            &log
        ));

        // OR within a position.
        assert!(log_matches(
            &with_topics(vec![EthHashList::List(vec![topic(9), topic(1)])]),
            &log
        ));
        assert!(!log_matches(
            &with_topics(vec![EthHashList::List(vec![topic(8), topic(9)])]),
            &log
        ));

        // AND across positions.
        assert!(log_matches(
            &with_topics(vec![
                EthHashList::Single(Some(topic(1))),
                EthHashList::Single(Some(topic(2))),
            ]),
            &log
        ));
        assert!(!log_matches(
            &with_topics(vec![
                EthHashList::Single(Some(topic(1))),
                EthHashList::Single(Some(topic(9))),
            ]),
            &log
        ));

        // More filter positions than log topics never match, even with wildcards
        // (go-ethereum's `filterLogs` semantics).
        assert!(!log_matches(
            &with_topics(vec![
                EthHashList::Single(Some(topic(1))),
                EthHashList::Single(Some(topic(2))),
                EthHashList::Single(None),
            ]),
            &log
        ));
    }

    #[test]
    fn log_matches_address_and_topics_combined() {
        let log = eth_log(&address_0(), vec![topic(1)]);
        let spec = EthFilterSpec {
            address: Some(vec![address_0()].into()),
            topics: Some(EthTopicSpec(vec![EthHashList::Single(Some(topic(1)))])),
            ..Default::default()
        };
        assert!(log_matches(&spec, &log));

        let wrong_address = EthFilterSpec {
            address: Some(vec![address_1()].into()),
            ..spec.clone()
        };
        assert!(!log_matches(&wrong_address, &log));

        let wrong_topic = EthFilterSpec {
            topics: Some(EthTopicSpec(vec![EthHashList::Single(Some(topic(9)))])),
            ..spec
        };
        assert!(!log_matches(&wrong_topic, &log));
    }
}
