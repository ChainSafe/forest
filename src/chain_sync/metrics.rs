// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use prometheus_client::{
    encoding::EncodeLabelSet,
    metrics::{counter::Counter, family::Family, histogram::Histogram},
};
use std::sync::LazyLock;

pub static TIPSET_PROCESSING_TIME: LazyLock<Histogram> = LazyLock::new(|| {
    let metric = crate::metrics::default_histogram();
    crate::metrics::default_registry().register(
        "tipset_processing_time",
        "Duration of routine which processes Tipsets to include them in the store",
        metric.clone(),
    );
    metric
});
pub static BLOCK_VALIDATION_TIME: LazyLock<Histogram> = LazyLock::new(|| {
    let metric = crate::metrics::default_histogram();
    crate::metrics::default_registry().register(
        "block_validation_time",
        "Duration of routine which validate blocks with no cache hit",
        metric.clone(),
    );
    metric
});
pub static LIBP2P_MESSAGE_TOTAL: LazyLock<Family<Libp2pMessageKindLabel, Counter>> =
    LazyLock::new(|| {
        let metric = Family::default();
        crate::metrics::default_registry().register(
            "libp2p_messsage_total",
            "Total number of libp2p messages by type",
            metric.clone(),
        );
        metric
    });
pub static INVALID_TIPSET_TOTAL: LazyLock<Counter> = LazyLock::new(|| {
    let metric = Counter::default();
    crate::metrics::default_registry().register(
        "invalid_tipset_total",
        "Total number of invalid tipsets received over gossipsub",
        metric.clone(),
    );
    metric
});
pub static GOSSIP_BLOCK_REJECTED_TOTAL: LazyLock<Family<GossipRejectReasonLabel, Counter>> =
    LazyLock::new(|| {
        let metric = Family::default();
        crate::metrics::default_registry().register(
            "gossip_block_rejected_total",
            "Total number of gossip blocks rejected by pre-validation",
            metric.clone(),
        );
        metric
    });

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet, derive_more::Constructor)]
pub struct Libp2pMessageKindLabel {
    libp2p_message_kind: &'static str,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet, derive_more::Constructor)]
pub struct GossipRejectReasonLabel {
    pub reason: &'static str,
}

pub mod values {
    use super::Libp2pMessageKindLabel;

    // libp2p_message_total
    pub const HELLO_REQUEST_INBOUND: Libp2pMessageKindLabel =
        Libp2pMessageKindLabel::new("hello_request_in");
    pub const HELLO_RESPONSE_OUTBOUND: Libp2pMessageKindLabel =
        Libp2pMessageKindLabel::new("hello_response_out");
    pub const HELLO_REQUEST_OUTBOUND: Libp2pMessageKindLabel =
        Libp2pMessageKindLabel::new("hello_request_out");
    pub const HELLO_RESPONSE_INBOUND: Libp2pMessageKindLabel =
        Libp2pMessageKindLabel::new("hello_response_in");
    pub const PEER_CONNECTED: Libp2pMessageKindLabel =
        Libp2pMessageKindLabel::new("peer_connected");
    pub const PEER_DISCONNECTED: Libp2pMessageKindLabel =
        Libp2pMessageKindLabel::new("peer_disconnected");
    pub const PUBSUB_BLOCK: Libp2pMessageKindLabel =
        Libp2pMessageKindLabel::new("pubsub_message_block");
    pub const PUBSUB_MESSAGE: Libp2pMessageKindLabel =
        Libp2pMessageKindLabel::new("pubsub_message_message");
    pub const CHAIN_EXCHANGE_REQUEST_OUTBOUND: Libp2pMessageKindLabel =
        Libp2pMessageKindLabel::new("chain_exchange_request_out");
    pub const CHAIN_EXCHANGE_RESPONSE_INBOUND: Libp2pMessageKindLabel =
        Libp2pMessageKindLabel::new("chain_exchange_response_in");
    pub const CHAIN_EXCHANGE_REQUEST_INBOUND: Libp2pMessageKindLabel =
        Libp2pMessageKindLabel::new("chain_exchange_request_in");
    pub const CHAIN_EXCHANGE_RESPONSE_OUTBOUND: Libp2pMessageKindLabel =
        Libp2pMessageKindLabel::new("chain_exchange_response_out");
}
